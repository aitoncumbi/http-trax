//! Data model shared by the desktop app, the MCP server and the on-disk format.
//!
//! Everything serialises as camelCase so the same structs are used for the
//! YAML files in a workspace and for the JSON sent to the UI.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Bumped when the on-disk layout changes incompatibly.
pub const FORMAT_VERSION: u32 = 1;

fn default_true() -> bool {
    true
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// `trax.yaml` at the workspace root. Its presence marks a directory as a
/// workspace. Shared with the team through git.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceManifest {
    pub version: u32,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Team that owns this workspace (free text, shown in the switcher).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    /// Workspace-wide variables (lowest precedence).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Variable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyValue {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl KeyValue {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
            description: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variable {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Secret values are never written to the shared file; they live in the
    /// workspace's local `.trax/` state instead.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
}

impl Variable {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
            secret: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultipartField {
    pub key: String,
    #[serde(default)]
    pub value: String,
    /// When true, `value` is a file path (relative to the workspace root or absolute).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_file: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum Body {
    #[default]
    None,
    Json {
        #[serde(default)]
        text: String,
    },
    Text {
        #[serde(default)]
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
    },
    Xml {
        #[serde(default)]
        text: String,
    },
    FormUrlencoded {
        #[serde(default)]
        fields: Vec<KeyValue>,
    },
    Multipart {
        #[serde(default)]
        fields: Vec<MultipartField>,
    },
    Graphql {
        #[serde(default)]
        query: String,
        #[serde(default)]
        variables: String,
    },
}

impl Body {
    pub fn is_none(&self) -> bool {
        matches!(self, Body::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Auth {
    /// Use the closest parent's auth (folder, then collection).
    #[default]
    Inherit,
    None,
    Bearer {
        #[serde(default)]
        token: String,
    },
    Basic {
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
    },
    ApiKey {
        #[serde(default)]
        key: String,
        #[serde(default)]
        value: String,
        #[serde(default)]
        location: ApiKeyLocation,
    },
    /// OAuth 2.0: the runner gets, caches and refreshes the access token.
    #[serde(rename = "oauth2")]
    OAuth2(Box<OAuth2>),
}

/// How an OAuth 2.0 access token is obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OAuth2Grant {
    /// Machine to machine; the runner fetches tokens by itself.
    #[default]
    ClientCredentials,
    /// The user signs in in the browser; PKCE and a loopback redirect.
    AuthorizationCode,
    /// The user enters a code on another device or in the browser.
    DeviceCode,
    /// Resource owner password (legacy, but common for internal APIs).
    Password,
}

/// Where the client id and secret go in token requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OAuth2ClientAuth {
    /// `Authorization: Basic …` (client_secret_basic).
    #[default]
    Basic,
    /// `client_id` and `client_secret` form fields (client_secret_post).
    Body,
}

impl OAuth2ClientAuth {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// OAuth 2.0 settings. Every text field takes `{{variables}}`, so token URLs
/// and client ids can differ per environment and secrets stay out of git.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2 {
    #[serde(default)]
    pub grant: OAuth2Grant,
    /// Authorization endpoint (authorization code).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub auth_url: String,
    /// Device authorization endpoint (device code).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub device_auth_url: String,
    #[serde(default)]
    pub token_url: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_secret: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    /// Loopback redirect for the authorization code grant, e.g.
    /// `http://127.0.0.1:7777/callback`. Empty: a random free port.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub redirect_uri: String,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub pkce: bool,
    /// Password grant.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    #[serde(default, skip_serializing_if = "OAuth2ClientAuth::is_default")]
    pub client_auth: OAuth2ClientAuth,
    /// Extra parameters for the authorization and token requests
    /// (`audience`, `resource`, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_params: Vec<KeyValue>,
    /// Scheme in the Authorization header; `Bearer` when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub header_prefix: String,
}

impl OAuth2 {
    pub fn new(grant: OAuth2Grant) -> Self {
        Self {
            grant,
            auth_url: String::new(),
            device_auth_url: String::new(),
            token_url: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            scope: String::new(),
            redirect_uri: String::new(),
            pkce: true,
            username: String::new(),
            password: String::new(),
            client_auth: OAuth2ClientAuth::Basic,
            extra_params: Vec::new(),
            header_prefix: String::new(),
        }
    }
}

/// An access token, cached in `.trax/state.json` (never committed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthToken {
    pub access_token: String,
    #[serde(default)]
    pub token_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    pub obtained_at: chrono::DateTime<chrono::Utc>,
}

impl Auth {
    pub fn is_inherit(&self) -> bool {
        matches!(self, Auth::Inherit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scripts {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pre_request: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub post_response: String,
}

impl Scripts {
    pub fn is_empty(&self) -> bool {
        self.pre_request.trim().is_empty() && self.post_response.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default = "default_true")]
    pub follow_redirects: bool,
    #[serde(default = "default_true")]
    pub verify_tls: bool,
    /// Send cookies from the workspace cookie jar and store the ones the
    /// response sets.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub cookies: bool,
}

impl Default for RequestSettings {
    fn default() -> Self {
        Self {
            timeout_ms: None,
            follow_redirects: true,
            verify_tls: true,
            cookies: true,
        }
    }
}

impl RequestSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A saved request (`*.req.yaml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub seq: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Body::is_none")]
    pub body: Body,
    #[serde(default, skip_serializing_if = "Auth::is_inherit")]
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "Scripts::is_empty")]
    pub scripts: Scripts,
    #[serde(default, skip_serializing_if = "RequestSettings::is_default")]
    pub settings: RequestSettings,
}

impl Request {
    pub fn new(name: impl Into<String>, method: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            seq: 0,
            description: String::new(),
            method: method.into(),
            url: url.into(),
            params: Vec::new(),
            headers: Vec::new(),
            body: Body::None,
            auth: Auth::Inherit,
            scripts: Scripts::default(),
            settings: RequestSettings::default(),
        }
    }
}

/// Metadata for a collection (`collection.yaml`) or folder (`folder.yaml`).
/// Headers, auth and scripts set here apply to every request below it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMeta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub seq: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Only meaningful on collections.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Variable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Auth::is_inherit")]
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "Scripts::is_empty")]
    pub scripts: Scripts,
}

impl GroupMeta {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            seq: 0,
            description: String::new(),
            variables: Vec::new(),
            headers: Vec::new(),
            auth: Auth::Inherit,
            scripts: Scripts::default(),
        }
    }
}

/// `environments/*.env.yaml` (shared through git) or
/// `.trax/environments/*.env.yaml` (this machine only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub id: String,
    pub name: String,
    /// Sending a request with this environment needs confirmation in the
    /// app, and agents may not run requests with it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub protected: bool,
    #[serde(default)]
    pub variables: Vec<Variable>,
}

fn is_true(v: &bool) -> bool {
    *v
}

fn is_false(v: &bool) -> bool {
    !*v
}

impl Environment {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            protected: false,
            variables: Vec::new(),
        }
    }
}

/// An environment as presented to the UI: the definition plus this
/// machine's local overrides (values set by scripts, "current values").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentView {
    #[serde(flatten)]
    pub environment: Environment,
    pub overrides: BTreeMap<String, String>,
    /// Stored in `environments/` and committed, rather than kept in `.trax/`.
    #[serde(default = "default_shared")]
    pub shared: bool,
}

fn default_shared() -> bool {
    true
}

// ------------------------------------------------------------------ tree

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestSummary {
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub seq: i64,
    /// Path relative to the workspace root.
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupNode {
    pub meta: GroupMeta,
    /// Directory relative to the workspace root.
    pub dir: String,
    pub items: Vec<TreeItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TreeItem {
    Folder(GroupNode),
    Request(RequestSummary),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTree {
    pub manifest: WorkspaceManifest,
    pub root: String,
    pub collections: Vec<GroupNode>,
    pub environments: Vec<EnvironmentView>,
    /// Files that exist but couldn't be parsed (for example while a git merge
    /// conflict is unresolved). The rest of the tree still loads.
    pub problems: Vec<LoadProblem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadProblem {
    pub path: String,
    pub message: String,
}

impl GroupNode {
    /// Every request below this node, depth-first in display order.
    pub fn requests(&self) -> Vec<&RequestSummary> {
        let mut out = Vec::new();
        fn walk<'a>(items: &'a [TreeItem], out: &mut Vec<&'a RequestSummary>) {
            for item in items {
                match item {
                    TreeItem::Request(r) => out.push(r),
                    TreeItem::Folder(f) => walk(&f.items, out),
                }
            }
        }
        walk(&self.items, &mut out);
        out
    }

    /// Find a group (this node or a descendant folder) by id.
    pub fn find_group(&self, id: &str) -> Option<&GroupNode> {
        if self.meta.id == id {
            return Some(self);
        }
        self.items.iter().find_map(|item| match item {
            TreeItem::Folder(f) => f.find_group(id),
            TreeItem::Request(_) => None,
        })
    }
}

// --------------------------------------------------------------- results

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub key: String,
    pub value: String,
}

impl Header {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// The request exactly as it went over the wire (after scripts and variables).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<Header>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseData {
    pub status: u16,
    pub status_text: String,
    pub http_version: String,
    pub headers: Vec<Header>,
    /// UTF-8 body. For binary payloads this is empty and `bodyBase64` is set.
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_base64: Option<String>,
    pub size: u64,
    pub time_ms: u64,
    /// Final URL after redirects.
    pub url: String,
}

impl ResponseData {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skipped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Where the test was declared: "request", "folder:<name>" or "collection:<name>".
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub level: String,
    pub message: String,
    pub phase: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptError {
    pub phase: String,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub id: String,
    pub request_id: Option<String>,
    pub request_name: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub request: SentRequest,
    pub response: Option<ResponseData>,
    /// Transport-level failure (DNS, TLS, timeout...).
    pub error: Option<String>,
    pub tests: Vec<TestResult>,
    pub logs: Vec<LogEntry>,
    pub script_errors: Vec<ScriptError>,
    /// A pre-request script called `trax.skipRequest()`.
    pub skipped: bool,
}

impl RunResult {
    pub fn tests_passed(&self) -> usize {
        self.tests.iter().filter(|t| t.passed && !t.skipped).count()
    }

    pub fn tests_failed(&self) -> usize {
        self.tests.iter().filter(|t| !t.passed).count()
    }

    /// No transport error, no script error and no failing test.
    pub fn ok(&self) -> bool {
        self.error.is_none() && self.script_errors.is_empty() && self.tests_failed() == 0
    }
}

/// A response saved against a request ("example"), stored in
/// `responses/<request-id>/<id>.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedResponse {
    pub id: String,
    pub request_id: String,
    pub name: String,
    pub saved_at: chrono::DateTime<chrono::Utc>,
    pub request: SentRequest,
    pub response: ResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionRunItem {
    pub request_id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub time_ms: Option<u64>,
    pub passed: usize,
    pub failed: usize,
    pub error: Option<String>,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionRunReport {
    pub group_id: String,
    pub group_name: String,
    pub environment: Option<String>,
    pub items: Vec<CollectionRunItem>,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub tests_passed: usize,
    pub tests_failed: usize,
    pub duration_ms: u64,
}
