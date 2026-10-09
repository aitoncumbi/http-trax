//! On-disk workspace layout.
//!
//! ```text
//! <root>/
//!   trax.yaml                        workspace manifest (marks the directory)
//!   .gitignore                       ignores .trax/
//!   .trax/state.json                 local only: runtime vars, overrides, secrets
//!   .trax/environments/<slug>.env.yaml   environments kept off git
//!   collections/<slug>/collection.yaml
//!   collections/<slug>/<request>.req.yaml
//!   collections/<slug>/<folder>/folder.yaml
//!   environments/<slug>.env.yaml
//!   responses/<request-id>/<response-id>.yaml
//! ```
//!
//! One small file per request keeps git diffs readable and merge conflicts
//! local to the request that actually changed.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, IoContext, Result};
use crate::fsutil::{read_json, read_yaml, rel, slugify, unique_path, write_json, write_yaml};
use crate::model::*;

pub const MANIFEST_FILE: &str = "trax.yaml";
pub const COLLECTIONS_DIR: &str = "collections";
pub const ENVIRONMENTS_DIR: &str = "environments";
pub const RESPONSES_DIR: &str = "responses";
pub const LOCAL_DIR: &str = ".trax";
const COLLECTION_FILE: &str = "collection.yaml";
const FOLDER_FILE: &str = "folder.yaml";
const REQUEST_SUFFIX: &str = ".req.yaml";
const ENV_SUFFIX: &str = ".env.yaml";
const STATE_FILE: &str = "state.json";

const GITIGNORE: &str = "# http-trax local state: runtime variables, secrets, history\n.trax/\n";

/// Machine-local state kept in `.trax/state.json` (git-ignored).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalState {
    /// Variables set by scripts with `trax.vars.set` (highest precedence).
    #[serde(default)]
    pub runtime_variables: BTreeMap<String, String>,
    /// Per-environment "current values" set by scripts (`trax.env.set`) or
    /// edited locally. They shadow the shared value without touching git.
    #[serde(default)]
    pub overrides: BTreeMap<String, BTreeMap<String, String>>,
    /// Values of variables marked `secret`, per environment id.
    #[serde(default)]
    pub secrets: BTreeMap<String, BTreeMap<String, String>>,
    /// OAuth 2.0 access tokens, by `oauth::Resolved::key`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub oauth_tokens: BTreeMap<String, OAuthToken>,
}

/// A request plus everything inherited from its parents.
#[derive(Debug, Clone)]
pub struct RequestContext {
    pub request: Request,
    pub path: PathBuf,
    /// Collection first, then each folder down to the request's parent.
    pub ancestors: Vec<GroupMeta>,
}

impl RequestContext {
    pub fn collection(&self) -> &GroupMeta {
        &self.ancestors[0]
    }
}

#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

/// Ids end up in file paths, so only allow a conservative character set.
fn check_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid(format!("invalid id {id:?}")))
    }
}

fn sort_key(seq: i64, name: &str) -> (i64, String) {
    (seq, name.to_lowercase())
}

impl Workspace {
    /// Create a new workspace in `root` (which may already exist, e.g. a
    /// service repository).
    pub fn init(root: impl AsRef<Path>, name: &str) -> Result<Self> {
        let root = std::path::absolute(root.as_ref()).at(root.as_ref())?;
        if root.join(MANIFEST_FILE).exists() {
            return Err(Error::Invalid(format!(
                "{} is already an http-trax workspace",
                root.display()
            )));
        }
        fs::create_dir_all(root.join(COLLECTIONS_DIR)).at(&root)?;
        fs::create_dir_all(root.join(ENVIRONMENTS_DIR)).at(&root)?;
        let manifest = WorkspaceManifest {
            version: FORMAT_VERSION,
            id: new_id(),
            name: name.to_string(),
            description: String::new(),
            team: None,
            variables: Vec::new(),
        };
        write_yaml(&root.join(MANIFEST_FILE), &manifest)?;

        let gitignore = root.join(".gitignore");
        let existing = fs::read_to_string(&gitignore).unwrap_or_default();
        if !existing
            .lines()
            .any(|l| l.trim() == ".trax/" || l.trim() == ".trax")
        {
            let mut text = existing;
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(GITIGNORE);
            fs::write(&gitignore, text).at(&gitignore)?;
        }
        Ok(Self { root })
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = std::path::absolute(root.as_ref()).at(root.as_ref())?;
        if !root.join(MANIFEST_FILE).is_file() {
            return Err(Error::NotFound(format!(
                "no {MANIFEST_FILE} in {}",
                root.display()
            )));
        }
        Ok(Self { root })
    }

    pub fn is_workspace(root: impl AsRef<Path>) -> bool {
        root.as_ref().join(MANIFEST_FILE).is_file()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> Result<WorkspaceManifest> {
        read_yaml(&self.root.join(MANIFEST_FILE))
    }

    pub fn save_manifest(&self, manifest: &WorkspaceManifest) -> Result<()> {
        write_yaml(&self.root.join(MANIFEST_FILE), manifest)
    }

    // ------------------------------------------------------------- tree

    pub fn tree(&self) -> Result<WorkspaceTree> {
        let manifest = self.manifest()?;
        let mut problems = Vec::new();
        let mut collections = Vec::new();
        let dir = self.root.join(COLLECTIONS_DIR);
        if dir.is_dir() {
            for entry in read_dir_sorted(&dir)? {
                if entry.is_dir() && entry.join(COLLECTION_FILE).is_file() {
                    if let Some(node) = self.load_group(&entry, COLLECTION_FILE, &mut problems) {
                        collections.push(node);
                    }
                }
            }
        }
        collections.sort_by_key(|c| sort_key(c.meta.seq, &c.meta.name));
        let environments = self.environments_collecting(&mut problems)?;
        Ok(WorkspaceTree {
            manifest,
            root: self.root.to_string_lossy().into_owned(),
            collections,
            environments,
            problems,
        })
    }

    fn load_group(
        &self,
        dir: &Path,
        meta_file: &str,
        problems: &mut Vec<LoadProblem>,
    ) -> Option<GroupNode> {
        let meta_path = dir.join(meta_file);
        let meta: GroupMeta = match read_yaml(&meta_path) {
            Ok(m) => m,
            Err(e) => {
                problems.push(self.problem(&meta_path, e));
                return None;
            }
        };
        let mut folders = Vec::new();
        let mut requests = Vec::new();
        let entries = match read_dir_sorted(dir) {
            Ok(e) => e,
            Err(e) => {
                problems.push(self.problem(dir, e));
                return None;
            }
        };
        for path in entries {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if path.join(FOLDER_FILE).is_file() {
                    if let Some(node) = self.load_group(&path, FOLDER_FILE, problems) {
                        folders.push(node);
                    }
                }
            } else if name.ends_with(REQUEST_SUFFIX) {
                match read_yaml::<Request>(&path) {
                    Ok(r) => requests.push(RequestSummary {
                        id: r.id,
                        name: r.name,
                        method: r.method,
                        url: r.url,
                        seq: r.seq,
                        path: rel(&self.root, &path),
                    }),
                    Err(e) => problems.push(self.problem(&path, e)),
                }
            }
        }
        // Folders and requests share one ordering (`seq`), so the order the
        // user arranges is also the order a collection run uses.
        let mut items: Vec<TreeItem> = folders.into_iter().map(TreeItem::Folder).collect();
        items.extend(requests.into_iter().map(TreeItem::Request));
        items.sort_by_key(|item| match item {
            TreeItem::Folder(f) => sort_key(f.meta.seq, &f.meta.name),
            TreeItem::Request(r) => sort_key(r.seq, &r.name),
        });
        Some(GroupNode {
            meta,
            dir: rel(&self.root, dir),
            items,
        })
    }

    fn problem(&self, path: &Path, err: Error) -> LoadProblem {
        tracing::warn!("skipping {}: {err}", path.display());
        LoadProblem {
            path: rel(&self.root, path),
            message: err.to_string(),
        }
    }

    fn collections(&self) -> Result<Vec<GroupNode>> {
        Ok(self.tree()?.collections)
    }

    fn find_group_dir(&self, id: &str) -> Result<(PathBuf, bool)> {
        for c in self.collections()? {
            if let Some(g) = c.find_group(id) {
                return Ok((self.root.join(&g.dir), g.meta.id == c.meta.id));
            }
        }
        Err(Error::NotFound(format!("collection or folder {id}")))
    }

    fn find_request_path(&self, id: &str) -> Result<PathBuf> {
        for c in self.collections()? {
            if let Some(r) = c.requests().into_iter().find(|r| r.id == id) {
                return Ok(self.root.join(&r.path));
            }
        }
        Err(Error::NotFound(format!("request {id}")))
    }

    // ------------------------------------------------------ collections

    pub fn create_collection(&self, name: &str) -> Result<GroupNode> {
        let base = self.root.join(COLLECTIONS_DIR);
        fs::create_dir_all(&base).at(&base)?;
        let existing = self.collections()?;
        let mut meta = GroupMeta::new(name);
        meta.seq = existing.iter().map(|c| c.meta.seq).max().unwrap_or(0) + 1;
        let dir = unique_path(&base, &slugify(name), "");
        fs::create_dir_all(&dir).at(&dir)?;
        write_yaml(&dir.join(COLLECTION_FILE), &meta)?;
        Ok(GroupNode {
            meta,
            dir: rel(&self.root, &dir),
            items: Vec::new(),
        })
    }

    pub fn create_folder(&self, parent_id: &str, name: &str) -> Result<GroupNode> {
        let (parent_dir, _) = self.find_group_dir(parent_id)?;
        let mut meta = GroupMeta::new(name);
        meta.seq = self.next_seq(&parent_dir)?;
        let dir = unique_path(&parent_dir, &slugify(name), "");
        fs::create_dir_all(&dir).at(&dir)?;
        write_yaml(&dir.join(FOLDER_FILE), &meta)?;
        Ok(GroupNode {
            meta,
            dir: rel(&self.root, &dir),
            items: Vec::new(),
        })
    }

    pub fn group(&self, id: &str) -> Result<GroupMeta> {
        let (dir, is_collection) = self.find_group_dir(id)?;
        read_yaml(&dir.join(if is_collection {
            COLLECTION_FILE
        } else {
            FOLDER_FILE
        }))
    }

    /// Update a collection's or folder's settings. Renaming also renames the
    /// directory so the repository stays browsable.
    pub fn update_group(&self, meta: &GroupMeta) -> Result<()> {
        let (dir, is_collection) = self.find_group_dir(&meta.id)?;
        let file = if is_collection {
            COLLECTION_FILE
        } else {
            FOLDER_FILE
        };
        let current: GroupMeta = read_yaml(&dir.join(file))?;
        let mut dir = dir;
        if slugify(&current.name) != slugify(&meta.name) {
            let parent = dir.parent().unwrap_or(&self.root).to_path_buf();
            let target = unique_path(&parent, &slugify(&meta.name), "");
            fs::rename(&dir, &target).at(&dir)?;
            dir = target;
        }
        write_yaml(&dir.join(file), meta)
    }

    pub fn delete_group(&self, id: &str) -> Result<()> {
        let mut target = None;
        for c in self.collections()? {
            if let Some(g) = c.find_group(id) {
                target = Some(g.clone());
                break;
            }
        }
        let group = target.ok_or_else(|| Error::NotFound(format!("collection or folder {id}")))?;
        for r in group.requests() {
            self.delete_responses_dir(&r.id)?;
        }
        let dir = self.root.join(&group.dir);
        fs::remove_dir_all(&dir).at(&dir)
    }

    fn next_seq(&self, dir: &Path) -> Result<i64> {
        let mut max = 0;
        for path in read_dir_sorted(dir)? {
            let seq = if path.is_dir() && path.join(FOLDER_FILE).is_file() {
                read_yaml::<GroupMeta>(&path.join(FOLDER_FILE))
                    .map(|m| m.seq)
                    .ok()
            } else if path.to_string_lossy().ends_with(REQUEST_SUFFIX) {
                read_yaml::<Request>(&path).map(|r| r.seq).ok()
            } else {
                None
            };
            max = max.max(seq.unwrap_or(0));
        }
        Ok(max + 1)
    }

    // --------------------------------------------------------- requests

    pub fn request(&self, id: &str) -> Result<Request> {
        read_yaml(&self.find_request_path(id)?)
    }

    /// The request plus its collection and folder chain (for inheritance).
    pub fn request_context(&self, id: &str) -> Result<RequestContext> {
        for c in self.collections()? {
            let mut chain = Vec::new();
            if let Some(path) = find_with_chain(&c, id, &mut chain) {
                let path = self.root.join(path);
                return Ok(RequestContext {
                    request: read_yaml(&path)?,
                    path,
                    ancestors: chain,
                });
            }
        }
        Err(Error::NotFound(format!("request {id}")))
    }

    /// Ancestors (collection first) of a collection or folder, including itself.
    pub fn group_chain(&self, id: &str) -> Result<Vec<GroupMeta>> {
        fn walk(node: &GroupNode, id: &str, chain: &mut Vec<GroupMeta>) -> bool {
            chain.push(node.meta.clone());
            if node.meta.id == id {
                return true;
            }
            for item in &node.items {
                if let TreeItem::Folder(f) = item {
                    if walk(f, id, chain) {
                        return true;
                    }
                }
            }
            chain.pop();
            false
        }
        for c in self.collections()? {
            let mut chain = Vec::new();
            if walk(&c, id, &mut chain) {
                return Ok(chain);
            }
        }
        Err(Error::NotFound(format!("collection or folder {id}")))
    }

    /// Add a request to a collection or folder. A missing id is generated and
    /// `seq` defaults to the end of the list.
    pub fn create_request(&self, parent_id: &str, mut request: Request) -> Result<Request> {
        let (dir, _) = self.find_group_dir(parent_id)?;
        if request.id.is_empty() {
            request.id = new_id();
        }
        check_id(&request.id)?;
        if self.find_request_path(&request.id).is_ok() {
            return Err(Error::Invalid(format!(
                "request id {} already exists",
                request.id
            )));
        }
        if request.seq == 0 {
            request.seq = self.next_seq(&dir)?;
        }
        normalise(&mut request);
        let path = unique_path(&dir, &slugify(&request.name), REQUEST_SUFFIX);
        write_yaml(&path, &request)?;
        Ok(request)
    }

    /// Overwrite an existing request. Renames the file when the name changes.
    pub fn save_request(&self, request: &Request) -> Result<()> {
        check_id(&request.id)?;
        let path = self.find_request_path(&request.id)?;
        let mut request = request.clone();
        normalise(&mut request);
        let stem = path
            .file_name()
            .map(|n| {
                n.to_string_lossy()
                    .trim_end_matches(REQUEST_SUFFIX)
                    .to_string()
            })
            .unwrap_or_default();
        let slug = slugify(&request.name);
        if !stem_matches(&stem, &slug) {
            let dir = path.parent().unwrap_or(&self.root);
            let target = unique_path(dir, &slug, REQUEST_SUFFIX);
            write_yaml(&target, &request)?;
            fs::remove_file(&path).at(&path)?;
            return Ok(());
        }
        write_yaml(&path, &request)
    }

    pub fn delete_request(&self, id: &str) -> Result<()> {
        let path = self.find_request_path(id)?;
        fs::remove_file(&path).at(&path)?;
        self.delete_responses_dir(id)
    }

    pub fn duplicate_request(&self, id: &str) -> Result<Request> {
        let ctx = self.request_context(id)?;
        let parent = ctx
            .ancestors
            .last()
            .expect("request has a parent")
            .id
            .clone();
        let mut copy = ctx.request.clone();
        copy.id = new_id();
        copy.name = format!("{} copy", ctx.request.name);
        copy.seq = 0;
        self.create_request(&parent, copy)
    }

    pub fn move_request(&self, id: &str, new_parent_id: &str) -> Result<()> {
        let path = self.find_request_path(id)?;
        let (dir, _) = self.find_group_dir(new_parent_id)?;
        if path.parent() == Some(dir.as_path()) {
            return Ok(());
        }
        let mut request: Request = read_yaml(&path)?;
        request.seq = self.next_seq(&dir)?;
        let target = unique_path(&dir, &slugify(&request.name), REQUEST_SUFFIX);
        write_yaml(&target, &request)?;
        fs::remove_file(&path).at(&path)
    }

    // ----------------------------------------------------- environments

    fn env_dir(&self, shared: bool) -> PathBuf {
        if shared {
            self.root.join(ENVIRONMENTS_DIR)
        } else {
            self.root.join(LOCAL_DIR).join(ENVIRONMENTS_DIR)
        }
    }

    /// Environment files with whether each is shared (committed).
    fn env_files(&self) -> Result<Vec<(PathBuf, bool)>> {
        let mut out = Vec::new();
        for shared in [true, false] {
            let dir = self.env_dir(shared);
            if !dir.is_dir() {
                continue;
            }
            out.extend(
                read_dir_sorted(&dir)?
                    .into_iter()
                    .filter(|p| p.to_string_lossy().ends_with(ENV_SUFFIX))
                    .map(|p| (p, shared)),
            );
        }
        Ok(out)
    }

    fn environments_collecting(
        &self,
        problems: &mut Vec<LoadProblem>,
    ) -> Result<Vec<EnvironmentView>> {
        let state = self.local_state()?;
        let mut out = Vec::new();
        for (path, shared) in self.env_files()? {
            match read_yaml::<Environment>(&path) {
                Ok(env) => out.push(self.view(env, shared, &state)),
                Err(e) => problems.push(self.problem(&path, e)),
            }
        }
        out.sort_by_key(|e| e.environment.name.to_lowercase());
        Ok(out)
    }

    fn view(&self, mut env: Environment, shared: bool, state: &LocalState) -> EnvironmentView {
        if let Some(secrets) = state.secrets.get(&env.id) {
            for var in env.variables.iter_mut().filter(|v| v.secret) {
                if let Some(value) = secrets.get(&var.key) {
                    var.value = value.clone();
                }
            }
        }
        let overrides = state.overrides.get(&env.id).cloned().unwrap_or_default();
        EnvironmentView {
            environment: env,
            overrides,
            shared,
        }
    }

    pub fn environments(&self) -> Result<Vec<EnvironmentView>> {
        self.environments_collecting(&mut Vec::new())
    }

    fn find_env_path(&self, id: &str) -> Result<Option<(PathBuf, bool)>> {
        for (path, shared) in self.env_files()? {
            if let Ok(env) = read_yaml::<Environment>(&path) {
                if env.id == id {
                    return Ok(Some((path, shared)));
                }
            }
        }
        Ok(None)
    }

    pub fn environment(&self, id: &str) -> Result<EnvironmentView> {
        let (path, shared) = self
            .find_env_path(id)?
            .ok_or_else(|| Error::NotFound(format!("environment {id}")))?;
        Ok(self.view(read_yaml(&path)?, shared, &self.local_state()?))
    }

    /// Find an environment by id or (case-insensitive) name.
    pub fn find_environment(&self, needle: &str) -> Result<EnvironmentView> {
        self.environments()?
            .into_iter()
            .find(|e| e.environment.id == needle || e.environment.name.eq_ignore_ascii_case(needle))
            .ok_or_else(|| Error::NotFound(format!("environment {needle}")))
    }

    /// Create or update an environment. Secret values are moved to local state.
    pub fn save_environment(&self, env: &Environment) -> Result<Environment> {
        let mut env = env.clone();
        if env.id.is_empty() {
            env.id = new_id();
        }
        check_id(&env.id)?;
        let mut shared = env.clone();
        let secrets: BTreeMap<String, String> = env
            .variables
            .iter()
            .filter(|v| v.secret)
            .map(|v| (v.key.clone(), v.value.clone()))
            .collect();
        for var in shared.variables.iter_mut().filter(|v| v.secret) {
            var.value.clear();
        }
        self.update_local_state(|state| {
            if secrets.is_empty() {
                state.secrets.remove(&env.id);
            } else {
                state.secrets.insert(env.id.clone(), secrets);
            }
        })?;
        let path = match self.find_env_path(&env.id)? {
            Some((path, _)) => path,
            None => unique_path(&self.env_dir(true), &slugify(&env.name), ENV_SUFFIX),
        };
        write_yaml(&path, &shared)?;
        Ok(env)
    }

    /// Move an environment between `environments/` (committed and shared with
    /// the team) and `.trax/environments/` (this machine only).
    pub fn set_environment_shared(&self, id: &str, shared: bool) -> Result<EnvironmentView> {
        let (path, is_shared) = self
            .find_env_path(id)?
            .ok_or_else(|| Error::NotFound(format!("environment {id}")))?;
        if is_shared != shared {
            let env: Environment = read_yaml(&path)?;
            let target = unique_path(&self.env_dir(shared), &slugify(&env.name), ENV_SUFFIX);
            write_yaml(&target, &env)?;
            fs::remove_file(&path).at(&path)?;
        }
        self.environment(id)
    }

    pub fn delete_environment(&self, id: &str) -> Result<()> {
        let (path, _) = self
            .find_env_path(id)?
            .ok_or_else(|| Error::NotFound(format!("environment {id}")))?;
        fs::remove_file(&path).at(&path)?;
        self.update_local_state(|state| {
            state.secrets.remove(id);
            state.overrides.remove(id);
        })
    }

    /// Effective variables of an environment: enabled shared values, then
    /// local overrides on top.
    pub fn effective_environment(&self, id: &str) -> Result<(String, BTreeMap<String, String>)> {
        let view = self.environment(id)?;
        let mut vars: BTreeMap<String, String> = view
            .environment
            .variables
            .iter()
            .filter(|v| v.enabled)
            .map(|v| (v.key.clone(), v.value.clone()))
            .collect();
        vars.extend(view.overrides);
        Ok((view.environment.name, vars))
    }

    // ------------------------------------------------------ local state

    fn state_path(&self) -> PathBuf {
        self.root.join(LOCAL_DIR).join(STATE_FILE)
    }

    pub fn local_state(&self) -> Result<LocalState> {
        let path = self.state_path();
        if !path.exists() {
            return Ok(LocalState::default());
        }
        read_json(&path)
    }

    pub fn update_local_state(&self, f: impl FnOnce(&mut LocalState)) -> Result<()> {
        let mut state = self.local_state()?;
        let before = state.clone();
        f(&mut state);
        if state != before {
            write_json(&self.state_path(), &state)?;
        }
        Ok(())
    }

    // -------------------------------------------------- saved responses

    fn responses_dir(&self, request_id: &str) -> Result<PathBuf> {
        check_id(request_id)?;
        Ok(self.root.join(RESPONSES_DIR).join(request_id))
    }

    fn delete_responses_dir(&self, request_id: &str) -> Result<()> {
        let dir = self.responses_dir(request_id)?;
        if dir.exists() {
            fs::remove_dir_all(&dir).at(&dir)?;
        }
        Ok(())
    }

    pub fn saved_responses(&self, request_id: &str) -> Result<Vec<SavedResponse>> {
        let dir = self.responses_dir(request_id)?;
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for path in read_dir_sorted(&dir)? {
            if path.extension().is_some_and(|e| e == "yaml") {
                match read_yaml::<SavedResponse>(&path) {
                    Ok(r) => out.push(r),
                    Err(e) => tracing::warn!("skipping saved response: {e}"),
                }
            }
        }
        out.sort_by_key(|r| std::cmp::Reverse(r.saved_at));
        Ok(out)
    }

    pub fn save_response(&self, response: &SavedResponse) -> Result<()> {
        check_id(&response.id)?;
        // Make sure the request exists so we don't create orphans.
        self.find_request_path(&response.request_id)?;
        let path = self
            .responses_dir(&response.request_id)?
            .join(format!("{}.yaml", response.id));
        write_yaml(&path, response)
    }

    pub fn delete_saved_response(&self, request_id: &str, response_id: &str) -> Result<()> {
        check_id(response_id)?;
        let path = self
            .responses_dir(request_id)?
            .join(format!("{response_id}.yaml"));
        if !path.exists() {
            return Err(Error::NotFound(format!("saved response {response_id}")));
        }
        fs::remove_file(&path).at(&path)
    }
}

fn find_with_chain(node: &GroupNode, id: &str, chain: &mut Vec<GroupMeta>) -> Option<String> {
    chain.push(node.meta.clone());
    for item in &node.items {
        match item {
            TreeItem::Request(r) if r.id == id => return Some(r.path.clone()),
            TreeItem::Folder(f) => {
                if let Some(p) = find_with_chain(f, id, chain) {
                    return Some(p);
                }
            }
            _ => {}
        }
    }
    chain.pop();
    None
}

/// `stem` is `slug` or `slug-<n>` (a de-duplicated name).
fn stem_matches(stem: &str, slug: &str) -> bool {
    match stem.strip_prefix(slug) {
        Some("") => true,
        Some(rest) => rest
            .strip_prefix('-')
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())),
        None => false,
    }
}

/// Tidy up a request before writing it.
fn normalise(request: &mut Request) {
    request.method = request.method.trim().to_uppercase();
    if request.method.is_empty() {
        request.method = "GET".into();
    }
}

fn read_dir_sorted(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .at(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Team A").unwrap();
        (dir, ws)
    }

    #[test]
    fn init_creates_layout() {
        let (dir, ws) = ws();
        assert!(dir.path().join("trax.yaml").is_file());
        assert!(dir.path().join("collections").is_dir());
        let gitignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains(".trax/"));
        assert_eq!(ws.manifest().unwrap().name, "Team A");
        assert!(Workspace::init(dir.path(), "again").is_err());
    }

    #[test]
    fn collections_folders_and_requests() {
        let (_dir, ws) = ws();
        let users = ws.create_collection("Users API").unwrap();
        let admin = ws.create_folder(&users.meta.id, "Admin").unwrap();
        let mut get = Request::new("Get user", "get", "{{baseUrl}}/users/1");
        get.scripts.post_response = "trax.test('ok', () => {\n  expect(1).toBe(1);\n});\n".into();
        let get = ws.create_request(&users.meta.id, get).unwrap();
        assert_eq!(get.method, "GET");
        let del = ws
            .create_request(
                &admin.meta.id,
                Request::new("Delete user", "DELETE", "/users/1"),
            )
            .unwrap();

        let tree = ws.tree().unwrap();
        assert!(tree.problems.is_empty());
        assert_eq!(tree.collections.len(), 1);
        let col = &tree.collections[0];
        assert_eq!(col.dir, "collections/users-api");
        assert_eq!(col.requests().len(), 2);
        // Items keep creation order (folder first here).
        assert!(matches!(col.items[0], TreeItem::Folder(_)));

        let ctx = ws.request_context(&del.id).unwrap();
        let names: Vec<_> = ctx.ancestors.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Users API", "Admin"]);

        // Multi-line scripts are stored as YAML block scalars (readable diffs).
        let text =
            fs::read_to_string(ws.root().join("collections/users-api/get-user.req.yaml")).unwrap();
        assert!(text.contains("postResponse: |"), "{text}");

        // Rename moves the file.
        let mut renamed = ws.request(&get.id).unwrap();
        renamed.name = "Fetch user".into();
        ws.save_request(&renamed).unwrap();
        assert!(ws
            .root()
            .join("collections/users-api/fetch-user.req.yaml")
            .is_file());
        assert!(!ws
            .root()
            .join("collections/users-api/get-user.req.yaml")
            .exists());

        let copy = ws.duplicate_request(&get.id).unwrap();
        assert_eq!(copy.name, "Fetch user copy");
        ws.move_request(&copy.id, &admin.meta.id).unwrap();
        assert_eq!(ws.request_context(&copy.id).unwrap().ancestors.len(), 2);

        ws.delete_group(&admin.meta.id).unwrap();
        assert_eq!(ws.tree().unwrap().collections[0].requests().len(), 1);
    }

    #[test]
    fn rename_collection_renames_dir() {
        let (_dir, ws) = ws();
        let c = ws.create_collection("Billing").unwrap();
        let mut meta = c.meta.clone();
        meta.name = "Payments".into();
        ws.update_group(&meta).unwrap();
        assert!(ws
            .root()
            .join("collections/payments/collection.yaml")
            .is_file());
        assert_eq!(ws.group(&c.meta.id).unwrap().name, "Payments");
    }

    #[test]
    fn broken_files_are_reported_not_fatal() {
        let (_dir, ws) = ws();
        let c = ws.create_collection("C").unwrap();
        ws.create_request(&c.meta.id, Request::new("ok", "GET", "/"))
            .unwrap();
        fs::write(
            ws.root().join("collections/c/bad.req.yaml"),
            "<<<<<<< HEAD\nid: x\n=======\nid: y\n>>>>>>> theirs\n",
        )
        .unwrap();
        let tree = ws.tree().unwrap();
        assert_eq!(tree.collections[0].requests().len(), 1);
        assert_eq!(tree.problems.len(), 1);
        assert_eq!(tree.problems[0].path, "collections/c/bad.req.yaml");
    }

    #[test]
    fn environments_keep_secrets_local() {
        let (_dir, ws) = ws();
        let mut env = Environment::new("Dev");
        env.variables
            .push(Variable::new("baseUrl", "http://localhost:8080"));
        let mut secret = Variable::new("apiKey", "s3cr3t");
        secret.secret = true;
        env.variables.push(secret);
        let env = ws.save_environment(&env).unwrap();

        let shared = fs::read_to_string(ws.root().join("environments/dev.env.yaml")).unwrap();
        assert!(!shared.contains("s3cr3t"));
        let view = ws.environment(&env.id).unwrap();
        assert_eq!(view.environment.variables[1].value, "s3cr3t");

        ws.update_local_state(|s| {
            s.overrides
                .entry(env.id.clone())
                .or_default()
                .insert("baseUrl".into(), "http://127.0.0.1:9000".into());
        })
        .unwrap();
        let (name, vars) = ws.effective_environment(&env.id).unwrap();
        assert_eq!(name, "Dev");
        assert_eq!(vars["baseUrl"], "http://127.0.0.1:9000");
        assert_eq!(ws.find_environment("dev").unwrap().environment.id, env.id);
    }

    #[test]
    fn environments_can_stay_off_git_and_be_protected() {
        let (_dir, ws) = ws();
        let mut env = Environment::new("Production");
        env.protected = true;
        env.variables
            .push(Variable::new("baseUrl", "https://api.example.com"));
        let env = ws.save_environment(&env).unwrap();
        let view = ws.environment(&env.id).unwrap();
        assert!(view.shared && view.environment.protected);
        let shared_path = ws.root().join("environments/production.env.yaml");
        assert!(fs::read_to_string(&shared_path)
            .unwrap()
            .contains("protected: true"));

        let view = ws.set_environment_shared(&env.id, false).unwrap();
        assert!(!view.shared);
        assert!(!shared_path.exists());
        assert!(ws
            .root()
            .join(".trax/environments/production.env.yaml")
            .exists());
        // Edits stay where the environment lives.
        let mut edited = view.environment.clone();
        edited.variables[0].value = "https://api2.example.com".into();
        ws.save_environment(&edited).unwrap();
        assert!(!shared_path.exists());
        assert_eq!(ws.environments().unwrap().len(), 1);
        assert_eq!(
            ws.effective_environment(&env.id).unwrap().1["baseUrl"],
            "https://api2.example.com"
        );

        let view = ws.set_environment_shared(&env.id, true).unwrap();
        assert!(view.shared && shared_path.exists());
        assert!(!ws
            .root()
            .join(".trax/environments/production.env.yaml")
            .exists());
        ws.delete_environment(&env.id).unwrap();
        assert!(ws.environments().unwrap().is_empty());
    }

    #[test]
    fn saved_responses_round_trip() {
        let (_dir, ws) = ws();
        let c = ws.create_collection("C").unwrap();
        let r = ws
            .create_request(&c.meta.id, Request::new("r", "GET", "/"))
            .unwrap();
        let saved = SavedResponse {
            id: new_id(),
            request_id: r.id.clone(),
            name: "200 OK".into(),
            saved_at: chrono::Utc::now(),
            request: SentRequest::default(),
            response: ResponseData {
                status: 200,
                status_text: "OK".into(),
                http_version: "HTTP/1.1".into(),
                headers: vec![],
                body: "{\n  \"ok\": true\n}".into(),
                body_base64: None,
                size: 16,
                time_ms: 3,
                url: "http://x/".into(),
            },
        };
        ws.save_response(&saved).unwrap();
        assert_eq!(ws.saved_responses(&r.id).unwrap(), vec![saved.clone()]);
        ws.delete_saved_response(&r.id, &saved.id).unwrap();
        assert!(ws.saved_responses(&r.id).unwrap().is_empty());
        assert!(ws.saved_responses("../etc").is_err());
    }
}
