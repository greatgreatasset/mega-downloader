//! Parsing of MEGA share links.
//!
//! Supported forms (new-style `mega.nz` links):
//!   - Folder:        `https://mega.nz/folder/<id>#<key>`
//!   - File:          `https://mega.nz/file/<id>#<key>`
//!   - File-in-folder:`https://mega.nz/folder/<id>#<key>/file/<handle>`
//!
//! The fragment after `#` is the base64 decryption key embedded in the link —
//! this is what makes a public-link downloader cryptographically legitimate.

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MegaLink {
    /// A shared folder, optionally pointing at a single file within it.
    Folder {
        id: String,
        key: String,
        /// Handle of a specific file inside the folder, if the link targets one.
        file: Option<String>,
    },
    /// A standalone shared file.
    File { id: String, key: String },
}

/// Parse a MEGA share link into its identifier(s) and decryption key.
pub fn parse(input: &str) -> Result<MegaLink> {
    let s = input.trim();

    // Strip scheme + host down to the path + fragment, tolerating both
    // `https://mega.nz/...` and bare `mega.nz/...` / `/folder/...` forms.
    let rest = s
        .split_once("mega.nz/")
        .map(|(_, r)| r)
        .or_else(|| s.split_once("mega.co.nz/").map(|(_, r)| r))
        .unwrap_or(s)
        .trim_start_matches('/');

    let (path, fragment) = match rest.split_once('#') {
        Some((p, f)) => (p, f),
        None => return Err(Error::InvalidLink(format!("missing '#' key: {input}"))),
    };

    if let Some(id) = path.strip_prefix("folder/") {
        // fragment may itself be `<key>/file/<handle>`
        let (key, file) = match fragment.split_once("/file/") {
            Some((k, h)) => (k, Some(h.to_string())),
            None => (fragment, None),
        };
        return finish(id, key).map(|(id, key)| MegaLink::Folder { id, key, file });
    }

    if let Some(id) = path.strip_prefix("file/") {
        return finish(id, fragment).map(|(id, key)| MegaLink::File { id, key });
    }

    Err(Error::InvalidLink(format!("unrecognized path: {input}")))
}

/// Validate and normalize the id/key pair extracted from a link.
fn finish(id: &str, key: &str) -> Result<(String, String)> {
    let id = id.trim_matches('/');
    let key = key.trim_matches('/');
    if id.is_empty() || key.is_empty() {
        return Err(Error::InvalidLink("empty id or key".into()));
    }
    Ok((id.to_string(), key.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_folder() {
        let l = parse("https://mega.nz/folder/AbCd1234#someKeyValue").unwrap();
        assert_eq!(
            l,
            MegaLink::Folder {
                id: "AbCd1234".into(),
                key: "someKeyValue".into(),
                file: None,
            }
        );
    }

    #[test]
    fn parses_file_in_folder() {
        let l = parse("https://mega.nz/folder/AbCd1234#someKey/file/XyZ987").unwrap();
        assert_eq!(
            l,
            MegaLink::Folder {
                id: "AbCd1234".into(),
                key: "someKey".into(),
                file: Some("XyZ987".into()),
            }
        );
    }

    #[test]
    fn parses_standalone_file() {
        let l = parse("mega.nz/file/FileId01#fileKey").unwrap();
        assert_eq!(
            l,
            MegaLink::File {
                id: "FileId01".into(),
                key: "fileKey".into(),
            }
        );
    }

    #[test]
    fn rejects_missing_key() {
        assert!(parse("https://mega.nz/folder/AbCd1234").is_err());
    }
}
