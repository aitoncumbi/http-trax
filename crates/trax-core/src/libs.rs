use std::collections::HashMap;
use std::path::PathBuf;
use crate::fsutil::write_atomic;

const LIBRARIES: &[(&str, &str)] = &[
    ("lodash", "https://cdn.jsdelivr.net/npm/lodash@4.17.21/lodash.min.js"),
    ("dayjs", "https://cdn.jsdelivr.net/npm/dayjs@1.11.13/dayjs.min.js"),
    ("ajv", "https://cdnjs.cloudflare.com/ajax/libs/ajv/8.12.0/ajv7.min.js"),
];

pub async fn fetch_libs(source: &str, cache_dir: PathBuf) -> HashMap<String, String> {
    let mut fetched = HashMap::new();
    for (name, url) in LIBRARIES {
        let pattern = format!("require('{name}')");
        let pattern2 = format!("require(\"{name}\")");
        if source.contains(&pattern) || source.contains(&pattern2) {
            let lib_path = cache_dir.join(format!("{name}.js"));
            if let Ok(content) = std::fs::read_to_string(&lib_path) {
                fetched.insert(name.to_string(), content);
            } else {
                if let Ok(resp) = reqwest::get(*url).await {
                    if let Ok(content) = resp.text().await {
                        let _ = write_atomic(&lib_path, content.as_bytes());
                        fetched.insert(name.to_string(), content);
                    }
                }
            }
        }
    }
    fetched
}
