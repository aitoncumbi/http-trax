//! Semantic 3-way merge for workspace YAML files (requests, collections,
//! folders, environments, and workspace manifests).
//!
//! Instead of failing when git's line-based merge sees nearby edits in a YAML
//! file, this module parses `base`, `ours`, and `theirs` into structured
//! models and merges individual fields (headers, query params, variables,
//! JSON body keys, pre-request & post-response scripts).
//!
//! When both sides changed *different* fields (or added different headers,
//! params, or tests), `can_auto_merge` is `true` and the merged YAML can be
//! written without any manual conflict resolution. When both sides changed
//! the *same* field to different values, a [`FieldConflict`] is reported for
//! the in-app conflict editor, and user choices (`Ours`, `Theirs`, `Base`)
//! produce the final merged YAML.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::diff::{file_kind, FileKind};
use crate::model::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MergeSide {
    Base,
    Ours,
    Theirs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldConflict {
    /// Deterministic identifier used as key in `choices` (e.g. `url`, `headers.Authorization`).
    pub id: String,
    /// UI section (`Request`, `Params`, `Headers`, `Body`, `Auth`, `Scripts`, `Variables`, `Settings`).
    pub section: String,
    /// Field or key name (`url`, `Authorization`, `postResponse`, ...).
    pub field: String,
    /// Human-readable description of the conflict.
    pub label: String,
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
    /// Currently selected side (`Ours` by default, or from `choices`).
    pub chosen: MergeSide,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticMergePreview {
    pub path: String,
    pub kind: FileKind,
    /// True when every field merged cleanly without overlapping edits.
    pub can_auto_merge: bool,
    /// Human-readable descriptions of fields merged automatically.
    pub auto_merged: Vec<String>,
    /// Field-level conflicts requiring a choice in the conflict editor.
    pub conflicts: Vec<FieldConflict>,
    /// Serialised YAML produced by applying `auto_merged` + `choices`.
    pub merged_yaml: String,
}

struct MergeCtx<'a> {
    choices: &'a BTreeMap<String, MergeSide>,
    auto_merged: Vec<String>,
    conflicts: Vec<FieldConflict>,
}

impl<'a> MergeCtx<'a> {
    fn new(choices: &'a BTreeMap<String, MergeSide>) -> Self {
        Self {
            choices,
            auto_merged: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    fn choice(&self, id: &str) -> MergeSide {
        self.choices.get(id).copied().unwrap_or(MergeSide::Ours)
    }

    #[allow(clippy::too_many_arguments)]
    fn scalar<T: Clone + PartialEq>(
        &mut self,
        id: &str,
        section: &str,
        field: &str,
        label: &str,
        base: &T,
        ours: &T,
        theirs: &T,
        fmt: impl Fn(&T) -> Option<String>,
    ) -> T {
        if ours == theirs {
            return ours.clone();
        }
        if ours == base {
            self.auto_merged.push(format!("{label} (theirs)"));
            return theirs.clone();
        }
        if theirs == base {
            self.auto_merged.push(format!("{label} (ours)"));
            return ours.clone();
        }
        let chosen = self.choice(id);
        self.conflicts.push(FieldConflict {
            id: id.to_string(),
            section: section.to_string(),
            field: field.to_string(),
            label: label.to_string(),
            base: fmt(base),
            ours: fmt(ours),
            theirs: fmt(theirs),
            chosen,
        });
        match chosen {
            MergeSide::Base => base.clone(),
            MergeSide::Ours => ours.clone(),
            MergeSide::Theirs => theirs.clone(),
        }
    }

    fn merge_key_values(
        &mut self,
        prefix: &str,
        section: &str,
        noun: &str,
        base: &[KeyValue],
        ours: &[KeyValue],
        theirs: &[KeyValue],
    ) -> Vec<KeyValue> {
        let b_map: BTreeMap<&str, &KeyValue> = base.iter().map(|k| (k.key.as_str(), k)).collect();
        let o_map: BTreeMap<&str, &KeyValue> = ours.iter().map(|k| (k.key.as_str(), k)).collect();
        let t_map: BTreeMap<&str, &KeyValue> = theirs.iter().map(|k| (k.key.as_str(), k)).collect();

        // Preserve order: ours keys first, then base keys, then new keys from theirs.
        let mut keys: Vec<String> = Vec::new();
        let mut seen = BTreeSet::new();
        for list in [ours, base, theirs] {
            for kv in list {
                if seen.insert(kv.key.clone()) {
                    keys.push(kv.key.clone());
                }
            }
        }

        let fmt_kv = |kv: Option<&&KeyValue>| -> Option<String> {
            kv.map(|k| {
                if k.enabled {
                    format!("{}: {}", k.key, k.value)
                } else {
                    format!("{}: {} (disabled)", k.key, k.value)
                }
            })
        };

        let mut out = Vec::new();
        for key in keys {
            let b = b_map.get(key.as_str());
            let o = o_map.get(key.as_str());
            let t = t_map.get(key.as_str());

            if o == t {
                if let Some(kv) = o {
                    out.push((*kv).clone());
                }
                continue;
            }
            if o == b {
                // Only theirs changed (added, updated, or removed).
                match t {
                    Some(kv) => {
                        self.auto_merged.push(format!("{noun} `{key}` (theirs)"));
                        out.push((*kv).clone());
                    }
                    None => {
                        self.auto_merged
                            .push(format!("{noun} `{key}` removed (theirs)"));
                    }
                }
                continue;
            }
            if t == b {
                // Only ours changed (added, updated, or removed).
                match o {
                    Some(kv) => {
                        self.auto_merged.push(format!("{noun} `{key}` (ours)"));
                        out.push((*kv).clone());
                    }
                    None => {
                        self.auto_merged
                            .push(format!("{noun} `{key}` removed (ours)"));
                    }
                }
                continue;
            }

            // Both sides modified or one modified and the other deleted.
            let id = format!("{prefix}.{key}");
            let chosen = self.choice(&id);
            self.conflicts.push(FieldConflict {
                id,
                section: section.to_string(),
                field: key.clone(),
                label: format!("{noun} `{key}`"),
                base: fmt_kv(b),
                ours: fmt_kv(o),
                theirs: fmt_kv(t),
                chosen,
            });
            let picked = match chosen {
                MergeSide::Base => b,
                MergeSide::Ours => o,
                MergeSide::Theirs => t,
            };
            if let Some(kv) = picked {
                out.push((*kv).clone());
            }
        }
        out
    }

    fn merge_variables(
        &mut self,
        prefix: &str,
        section: &str,
        base: &[Variable],
        ours: &[Variable],
        theirs: &[Variable],
    ) -> Vec<Variable> {
        let b_map: BTreeMap<&str, &Variable> = base.iter().map(|v| (v.key.as_str(), v)).collect();
        let o_map: BTreeMap<&str, &Variable> = ours.iter().map(|v| (v.key.as_str(), v)).collect();
        let t_map: BTreeMap<&str, &Variable> = theirs.iter().map(|v| (v.key.as_str(), v)).collect();

        let mut keys: Vec<String> = Vec::new();
        let mut seen = BTreeSet::new();
        for list in [ours, base, theirs] {
            for v in list {
                if seen.insert(v.key.clone()) {
                    keys.push(v.key.clone());
                }
            }
        }

        let fmt_v = |v: Option<&&Variable>| -> Option<String> {
            v.map(|x| {
                if x.secret {
                    format!("{}: (secret)", x.key)
                } else {
                    format!("{}: {}", x.key, x.value)
                }
            })
        };

        let mut out = Vec::new();
        for key in keys {
            let b = b_map.get(key.as_str());
            let o = o_map.get(key.as_str());
            let t = t_map.get(key.as_str());

            if o == t {
                if let Some(v) = o {
                    out.push((*v).clone());
                }
                continue;
            }
            if o == b {
                if let Some(v) = t {
                    self.auto_merged.push(format!("Variable `{key}` (theirs)"));
                    out.push((*v).clone());
                } else {
                    self.auto_merged
                        .push(format!("Variable `{key}` removed (theirs)"));
                }
                continue;
            }
            if t == b {
                if let Some(v) = o {
                    self.auto_merged.push(format!("Variable `{key}` (ours)"));
                    out.push((*v).clone());
                } else {
                    self.auto_merged
                        .push(format!("Variable `{key}` removed (ours)"));
                }
                continue;
            }

            let id = format!("{prefix}.{key}");
            let chosen = self.choice(&id);
            self.conflicts.push(FieldConflict {
                id,
                section: section.to_string(),
                field: key.clone(),
                label: format!("Variable `{key}`"),
                base: fmt_v(b),
                ours: fmt_v(o),
                theirs: fmt_v(t),
                chosen,
            });
            let picked = match chosen {
                MergeSide::Base => b,
                MergeSide::Ours => o,
                MergeSide::Theirs => t,
            };
            if let Some(v) = picked {
                out.push((*v).clone());
            }
        }
        out
    }

    fn merge_script(
        &mut self,
        id: &str,
        field: &str,
        label: &str,
        base: &str,
        ours: &str,
        theirs: &str,
    ) -> String {
        let b = base.trim_end();
        let o = ours.trim_end();
        let t = theirs.trim_end();
        if o == t {
            return ours.to_string();
        }
        if o == b {
            self.auto_merged.push(format!("{label} (theirs)"));
            return theirs.to_string();
        }
        if t == b {
            self.auto_merged.push(format!("{label} (ours)"));
            return ours.to_string();
        }
        // If both sides appended distinct blocks onto `base` (common when two
        // teammates each add a `trax.test(...)` assertion), combine both additions!
        if !b.is_empty() && o.starts_with(b) && t.starts_with(b) {
            let o_added = o[b.len()..].trim();
            let t_added = t[b.len()..].trim();
            if !o_added.is_empty() && !t_added.is_empty() && o_added != t_added {
                self.auto_merged
                    .push(format!("{label} (combined additions)"));
                return format!("{b}\n\n{o_added}\n\n{t_added}\n");
            }
        }
        let chosen = self.choice(id);
        self.conflicts.push(FieldConflict {
            id: id.to_string(),
            section: "Scripts".to_string(),
            field: field.to_string(),
            label: label.to_string(),
            base: (!b.is_empty()).then(|| b.to_string()),
            ours: (!o.is_empty()).then(|| o.to_string()),
            theirs: (!t.is_empty()).then(|| t.to_string()),
            chosen,
        });
        match chosen {
            MergeSide::Base => base.to_string(),
            MergeSide::Ours => ours.to_string(),
            MergeSide::Theirs => theirs.to_string(),
        }
    }

    fn merge_json_values(
        &mut self,
        path_prefix: &str,
        base: &serde_json::Value,
        ours: &serde_json::Value,
        theirs: &serde_json::Value,
    ) -> serde_json::Value {
        if ours == theirs {
            return ours.clone();
        }
        if ours == base {
            self.auto_merged
                .push(format!("JSON `{path_prefix}` (theirs)"));
            return theirs.clone();
        }
        if theirs == base {
            self.auto_merged
                .push(format!("JSON `{path_prefix}` (ours)"));
            return ours.clone();
        }
        if let (
            serde_json::Value::Object(b_obj),
            serde_json::Value::Object(o_obj),
            serde_json::Value::Object(t_obj),
        ) = (base, ours, theirs)
        {
            let mut keys = BTreeSet::new();
            for m in [o_obj, b_obj, t_obj] {
                for k in m.keys() {
                    keys.insert(k.clone());
                }
            }
            let mut merged = serde_json::Map::new();
            for k in keys {
                let child_path = if path_prefix.is_empty() || path_prefix == "body" {
                    format!("body.{k}")
                } else {
                    format!("{path_prefix}.{k}")
                };
                let b_v = b_obj.get(&k);
                let o_v = o_obj.get(&k);
                let t_v = t_obj.get(&k);
                if o_v == t_v {
                    if let Some(v) = o_v {
                        merged.insert(k, v.clone());
                    }
                    continue;
                }
                if o_v == b_v {
                    if let Some(v) = t_v {
                        self.auto_merged.push(format!("Body field `{k}` (theirs)"));
                        merged.insert(k, v.clone());
                    } else {
                        self.auto_merged
                            .push(format!("Body field `{k}` removed (theirs)"));
                    }
                    continue;
                }
                if t_v == b_v {
                    if let Some(v) = o_v {
                        self.auto_merged.push(format!("Body field `{k}` (ours)"));
                        merged.insert(k, v.clone());
                    } else {
                        self.auto_merged
                            .push(format!("Body field `{k}` removed (ours)"));
                    }
                    continue;
                }
                if let (Some(bv), Some(ov), Some(tv)) = (b_v, o_v, t_v) {
                    if bv.is_object() && ov.is_object() && tv.is_object() {
                        let sub = self.merge_json_values(&child_path, bv, ov, tv);
                        merged.insert(k, sub);
                        continue;
                    }
                }
                let chosen = self.choice(&child_path);
                self.conflicts.push(FieldConflict {
                    id: child_path,
                    section: "Body".to_string(),
                    field: k.clone(),
                    label: format!("Body field `{k}`"),
                    base: b_v.map(|v| v.to_string()),
                    ours: o_v.map(|v| v.to_string()),
                    theirs: t_v.map(|v| v.to_string()),
                    chosen,
                });
                let picked = match chosen {
                    MergeSide::Base => b_v,
                    MergeSide::Ours => o_v,
                    MergeSide::Theirs => t_v,
                };
                if let Some(v) = picked {
                    merged.insert(k, v.clone());
                }
            }
            return serde_json::Value::Object(merged);
        }

        let id = if path_prefix.is_empty() {
            "body".to_string()
        } else {
            path_prefix.to_string()
        };
        let chosen = self.choice(&id);
        self.conflicts.push(FieldConflict {
            id,
            section: "Body".to_string(),
            field: "json".to_string(),
            label: "JSON body".to_string(),
            base: Some(base.to_string()),
            ours: Some(ours.to_string()),
            theirs: Some(theirs.to_string()),
            chosen,
        });
        match chosen {
            MergeSide::Base => base.clone(),
            MergeSide::Ours => ours.clone(),
            MergeSide::Theirs => theirs.clone(),
        }
    }

    fn merge_body(&mut self, base: &Body, ours: &Body, theirs: &Body) -> Body {
        if ours == theirs {
            return ours.clone();
        }
        if ours == base {
            self.auto_merged.push("Body (theirs)".to_string());
            return theirs.clone();
        }
        if theirs == base {
            self.auto_merged.push("Body (ours)".to_string());
            return ours.clone();
        }
        match (base, ours, theirs) {
            (Body::Json { text: b }, Body::Json { text: o }, Body::Json { text: t }) => {
                if let (Ok(bv), Ok(ov), Ok(tv)) = (
                    serde_json::from_str::<serde_json::Value>(b),
                    serde_json::from_str::<serde_json::Value>(o),
                    serde_json::from_str::<serde_json::Value>(t),
                ) {
                    let merged = self.merge_json_values("body", &bv, &ov, &tv);
                    let text = serde_json::to_string_pretty(&merged).unwrap_or_else(|_| o.clone());
                    return Body::Json { text };
                }
            }
            (
                Body::FormUrlencoded { fields: b },
                Body::FormUrlencoded { fields: o },
                Body::FormUrlencoded { fields: t },
            ) => {
                let fields = self.merge_key_values("body.form", "Body", "Form field", b, o, t);
                return Body::FormUrlencoded { fields };
            }
            (
                Body::Graphql {
                    query: qb,
                    variables: vb,
                },
                Body::Graphql {
                    query: qo,
                    variables: vo,
                },
                Body::Graphql {
                    query: qt,
                    variables: vt,
                },
            ) => {
                let query = self.scalar(
                    "body.query",
                    "Body",
                    "query",
                    "GraphQL query",
                    qb,
                    qo,
                    qt,
                    |s| (!s.is_empty()).then(|| s.clone()),
                );
                let variables = self.scalar(
                    "body.variables",
                    "Body",
                    "variables",
                    "GraphQL variables",
                    vb,
                    vo,
                    vt,
                    |s| (!s.is_empty()).then(|| s.clone()),
                );
                return Body::Graphql { query, variables };
            }
            _ => {}
        }

        self.scalar(
            "body",
            "Body",
            "body",
            "Request body",
            base,
            ours,
            theirs,
            |b| {
                serde_yaml_ng::to_string(b)
                    .ok()
                    .map(|s| s.trim().to_string())
            },
        )
    }
}

fn parse_opt<T: serde::de::DeserializeOwned>(s: Option<&str>) -> Option<T> {
    let text = s?.trim();
    if text.is_empty() {
        return None;
    }
    serde_yaml_ng::from_str(text).ok()
}

/// Perform a semantic 3-way merge on a workspace YAML file (`*.req.yaml`,
/// `collection.yaml`, `folder.yaml`, `*.env.yaml`, `trax.yaml`).
///
/// Returns `None` if the file type is not a semantic workspace YAML file or if
/// `ours` and `theirs` cannot be parsed as valid YAML models.
pub fn semantic_merge(
    path: &str,
    base: Option<&str>,
    ours: Option<&str>,
    theirs: Option<&str>,
    choices: &BTreeMap<String, MergeSide>,
) -> Option<SemanticMergePreview> {
    let kind = file_kind(path);
    let mut ctx = MergeCtx::new(choices);

    let merged_yaml = match kind {
        FileKind::Request => {
            let o: Request = parse_opt(ours)?;
            let t: Request = parse_opt(theirs)?;
            let b: Request = parse_opt(base).unwrap_or_else(|| Request {
                id: o.id.clone(),
                ..Request::new("", "", "")
            });

            let name = ctx.scalar(
                "name",
                "Request",
                "name",
                "Request name",
                &b.name,
                &o.name,
                &t.name,
                |s| Some(s.clone()),
            );
            let method = ctx.scalar(
                "method",
                "Request",
                "method",
                "HTTP method",
                &b.method,
                &o.method,
                &t.method,
                |s| Some(s.clone()),
            );
            let url = ctx.scalar(
                "url",
                "Request",
                "url",
                "Request URL",
                &b.url,
                &o.url,
                &t.url,
                |s| Some(s.clone()),
            );
            let description = ctx.scalar(
                "description",
                "Docs",
                "description",
                "Description",
                &b.description,
                &o.description,
                &t.description,
                |s| (!s.is_empty()).then(|| s.clone()),
            );
            let seq = ctx.scalar(
                "seq",
                "Request",
                "seq",
                "Display order",
                &b.seq,
                &o.seq,
                &t.seq,
                |n| Some(n.to_string()),
            );
            let params = ctx.merge_key_values(
                "params",
                "Params",
                "Query param",
                &b.params,
                &o.params,
                &t.params,
            );
            let headers = ctx.merge_key_values(
                "headers", "Headers", "Header", &b.headers, &o.headers, &t.headers,
            );
            let body = ctx.merge_body(&b.body, &o.body, &t.body);
            let auth = ctx.scalar(
                "auth",
                "Auth",
                "auth",
                "Authentication",
                &b.auth,
                &o.auth,
                &t.auth,
                |a| {
                    serde_yaml_ng::to_string(a)
                        .ok()
                        .map(|s| s.trim().to_string())
                },
            );
            let pre_request = ctx.merge_script(
                "scripts.preRequest",
                "preRequest",
                "Pre-request script",
                &b.scripts.pre_request,
                &o.scripts.pre_request,
                &t.scripts.pre_request,
            );
            let post_response = ctx.merge_script(
                "scripts.postResponse",
                "postResponse",
                "Post-response script",
                &b.scripts.post_response,
                &o.scripts.post_response,
                &t.scripts.post_response,
            );
            let settings = ctx.scalar(
                "settings",
                "Settings",
                "settings",
                "Request settings",
                &b.settings,
                &o.settings,
                &t.settings,
                |s| {
                    serde_yaml_ng::to_string(s)
                        .ok()
                        .map(|x| x.trim().to_string())
                },
            );

            let merged = Request {
                id: o.id,
                name,
                seq,
                description,
                method,
                url,
                params,
                headers,
                body,
                auth,
                scripts: Scripts {
                    pre_request,
                    post_response,
                },
                settings,
            };
            serde_yaml_ng::to_string(&merged).ok()?
        }
        FileKind::Collection | FileKind::Folder => {
            let o: GroupMeta = parse_opt(ours)?;
            let t: GroupMeta = parse_opt(theirs)?;
            let b: GroupMeta = parse_opt(base).unwrap_or_else(|| GroupMeta {
                id: o.id.clone(),
                ..GroupMeta::new("")
            });
            let noun = if kind == FileKind::Collection {
                "Collection"
            } else {
                "Folder"
            };
            let name = ctx.scalar(
                "name",
                noun,
                "name",
                &format!("{noun} name"),
                &b.name,
                &o.name,
                &t.name,
                |s| Some(s.clone()),
            );
            let seq = ctx.scalar(
                "seq",
                noun,
                "seq",
                "Display order",
                &b.seq,
                &o.seq,
                &t.seq,
                |n| Some(n.to_string()),
            );
            let description = ctx.scalar(
                "description",
                "Docs",
                "description",
                "Description",
                &b.description,
                &o.description,
                &t.description,
                |s| (!s.is_empty()).then(|| s.clone()),
            );
            let variables = ctx.merge_variables(
                "variables",
                "Variables",
                &b.variables,
                &o.variables,
                &t.variables,
            );
            let headers = ctx.merge_key_values(
                "headers", "Headers", "Header", &b.headers, &o.headers, &t.headers,
            );
            let auth = ctx.scalar(
                "auth",
                "Auth",
                "auth",
                "Authentication",
                &b.auth,
                &o.auth,
                &t.auth,
                |a| {
                    serde_yaml_ng::to_string(a)
                        .ok()
                        .map(|s| s.trim().to_string())
                },
            );
            let pre_request = ctx.merge_script(
                "scripts.preRequest",
                "preRequest",
                "Pre-request script",
                &b.scripts.pre_request,
                &o.scripts.pre_request,
                &t.scripts.pre_request,
            );
            let post_response = ctx.merge_script(
                "scripts.postResponse",
                "postResponse",
                "Post-response script",
                &b.scripts.post_response,
                &o.scripts.post_response,
                &t.scripts.post_response,
            );
            let merged = GroupMeta {
                id: o.id,
                name,
                seq,
                description,
                variables,
                headers,
                auth,
                scripts: Scripts {
                    pre_request,
                    post_response,
                },
            };
            serde_yaml_ng::to_string(&merged).ok()?
        }
        FileKind::Environment => {
            let o: Environment = parse_opt(ours)?;
            let t: Environment = parse_opt(theirs)?;
            let b: Environment = parse_opt(base).unwrap_or_else(|| Environment {
                id: o.id.clone(),
                ..Environment::new("")
            });
            let name = ctx.scalar(
                "name",
                "Environment",
                "name",
                "Environment name",
                &b.name,
                &o.name,
                &t.name,
                |s| Some(s.clone()),
            );
            let protected = ctx.scalar(
                "protected",
                "Environment",
                "protected",
                "Protected environment",
                &b.protected,
                &o.protected,
                &t.protected,
                |p| Some(p.to_string()),
            );
            let variables = ctx.merge_variables(
                "variables",
                "Variables",
                &b.variables,
                &o.variables,
                &t.variables,
            );
            let merged = Environment {
                id: o.id,
                name,
                protected,
                variables,
            };
            serde_yaml_ng::to_string(&merged).ok()?
        }
        FileKind::Workspace => {
            let o: WorkspaceManifest = parse_opt(ours)?;
            let t: WorkspaceManifest = parse_opt(theirs)?;
            let b: WorkspaceManifest = parse_opt(base).unwrap_or_else(|| WorkspaceManifest {
                version: o.version,
                id: o.id.clone(),
                name: String::new(),
                description: String::new(),
                team: None,
                variables: Vec::new(),
            });
            let name = ctx.scalar(
                "name",
                "Workspace",
                "name",
                "Workspace name",
                &b.name,
                &o.name,
                &t.name,
                |s| Some(s.clone()),
            );
            let team = ctx.scalar(
                "team",
                "Workspace",
                "team",
                "Team name",
                &b.team,
                &o.team,
                &t.team,
                |s| s.clone(),
            );
            let description = ctx.scalar(
                "description",
                "Docs",
                "description",
                "Description",
                &b.description,
                &o.description,
                &t.description,
                |s| (!s.is_empty()).then(|| s.clone()),
            );
            let variables = ctx.merge_variables(
                "variables",
                "Variables",
                &b.variables,
                &o.variables,
                &t.variables,
            );
            let merged = WorkspaceManifest {
                version: o.version.max(t.version),
                id: o.id,
                name,
                description,
                team,
                variables,
            };
            serde_yaml_ng::to_string(&merged).ok()?
        }
        _ => return None,
    };

    Some(SemanticMergePreview {
        path: path.to_string(),
        kind,
        can_auto_merge: ctx.conflicts.is_empty(),
        auto_merged: ctx.auto_merged,
        conflicts: ctx.conflicts,
        merged_yaml,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_merges_disjoint_request_fields_and_json_body() {
        let mut base = Request::new("Create Order", "POST", "{{baseUrl}}/orders");
        base.headers = vec![KeyValue::new("Accept", "application/json")];
        base.params = vec![KeyValue::new("dryRun", "false")];
        base.body = Body::Json {
            text: r#"{"sku":"A-1","qty":1}"#.into(),
        };
        base.scripts.post_response =
            "trax.test(\"status is 200\", () => {\n  trax.expect(trax.response.status).toBe(200);\n});".into();

        // Alice adds an X-Idempotency-Key header, changes JSON qty to 2, and adds a second test.
        let mut ours = base.clone();
        ours.headers
            .push(KeyValue::new("X-Idempotency-Key", "{{$uuid}}"));
        ours.body = Body::Json {
            text: r#"{"sku":"A-1","qty":2}"#.into(),
        };
        ours.scripts.post_response.push_str(
            "\ntrax.test(\"has order id\", () => {\n  trax.expect(trax.response.json().id).toBeTruthy();\n});",
        );

        // Bob updates query param dryRun -> true, adds JSON currency field, and adds a third test.
        let mut theirs = base.clone();
        theirs.params[0].value = "true".into();
        theirs.body = Body::Json {
            text: r#"{"sku":"A-1","qty":1,"currency":"EUR"}"#.into(),
        };
        theirs.scripts.post_response.push_str(
            "\ntrax.test(\"fast response\", () => {\n  trax.expect(trax.response.time).toBeLessThan(500);\n});",
        );

        let b_yaml = serde_yaml_ng::to_string(&base).unwrap();
        let o_yaml = serde_yaml_ng::to_string(&ours).unwrap();
        let t_yaml = serde_yaml_ng::to_string(&theirs).unwrap();

        let preview = semantic_merge(
            "collections/orders/create.req.yaml",
            Some(&b_yaml),
            Some(&o_yaml),
            Some(&t_yaml),
            &BTreeMap::new(),
        )
        .unwrap();

        assert!(
            preview.can_auto_merge,
            "expected clean auto-merge: {preview:?}"
        );
        assert!(preview.conflicts.is_empty());

        let merged: Request = serde_yaml_ng::from_str(&preview.merged_yaml).unwrap();
        assert_eq!(merged.headers.len(), 2);
        assert_eq!(merged.params[0].value, "true");
        assert!(merged.scripts.post_response.contains("has order id"));
        assert!(merged.scripts.post_response.contains("fast response"));
        if let Body::Json { text } = &merged.body {
            let v: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(v["qty"], 2);
            assert_eq!(v["currency"], "EUR");
        } else {
            panic!("expected JSON body");
        }
    }

    #[test]
    fn reports_field_conflict_and_applies_user_choice() {
        let mut base = Request::new("Get User", "GET", "{{baseUrl}}/v1/users");
        base.headers = vec![KeyValue::new("X-Version", "1")];

        let mut ours = base.clone();
        ours.url = "{{baseUrl}}/v2/users".into();
        ours.headers[0].value = "2-ours".into();

        let mut theirs = base.clone();
        theirs.url = "{{baseUrl}}/v3/users".into();
        theirs.headers[0].value = "2-theirs".into();

        let b_yaml = serde_yaml_ng::to_string(&base).unwrap();
        let o_yaml = serde_yaml_ng::to_string(&ours).unwrap();
        let t_yaml = serde_yaml_ng::to_string(&theirs).unwrap();

        let preview = semantic_merge(
            "users.req.yaml",
            Some(&b_yaml),
            Some(&o_yaml),
            Some(&t_yaml),
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(!preview.can_auto_merge);
        assert_eq!(preview.conflicts.len(), 2);

        let mut choices = BTreeMap::new();
        choices.insert("url".to_string(), MergeSide::Theirs);
        choices.insert("headers.X-Version".to_string(), MergeSide::Ours);

        let resolved = semantic_merge(
            "users.req.yaml",
            Some(&b_yaml),
            Some(&o_yaml),
            Some(&t_yaml),
            &choices,
        )
        .unwrap();
        let merged: Request = serde_yaml_ng::from_str(&resolved.merged_yaml).unwrap();
        assert_eq!(merged.url, "{{baseUrl}}/v3/users");
        assert_eq!(merged.headers[0].value, "2-ours");
    }
}
