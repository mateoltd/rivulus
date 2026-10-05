//! Multipart helpers (`payload_json` first part + files; sticker form-fields variant).
//!
//! All builders are sync and do no IO. `payload_json` is always the first
//! part; file parts follow as `files[N]`. Callers place attachment
//! descriptions inside `payload_json` (`attachments` array); the
//! [`MultipartFile::description`] field is carried for caller convenience.

/// File part for multipart uploads.
#[derive(Debug, Clone)]
pub struct MultipartFile {
    /// Filename (sanitized via [`common::validate::filename`]; no slashes/CRLF).
    pub filename: Box<str>,
    /// MIME type (e.g. `image/png`).
    pub content_type: String,
    /// Raw bytes.
    pub bytes: Vec<u8>,
    /// Optional attachment description (belongs in `payload_json`; kept here).
    pub description: Option<Box<str>>,
}

impl MultipartFile {
    /// New file part.
    #[must_use]
    pub fn new(
        filename: Box<str>,
        content_type: String,
        bytes: Vec<u8>,
        description: Option<Box<str>>,
    ) -> Self {
        Self {
            filename,
            content_type,
            bytes,
            description,
        }
    }
}

/// Build a message/webhook/interaction multipart form.
///
/// `payload_json` must be UTF-8 JSON bytes and becomes the FIRST part named
/// `payload_json` (`application/json`). Each file becomes `files[N]`.
///
/// # Errors
/// Returns [`common::Error::Validation`] on bad filenames, non-UTF8
/// `payload_json`, or invalid MIME types.
pub fn message_form(
    payload_json: &[u8],
    files: &[MultipartFile],
) -> Result<reqwest::multipart::Form, common::Error> {
    let json_str = core::str::from_utf8(payload_json)
        .map_err(|_| common::Error::Validation(Box::from("payload_json must be UTF-8")))?;
    let payload_part = reqwest::multipart::Part::bytes(json_str.as_bytes().to_vec())
        .mime_str("application/json")
        .map_err(|_| common::Error::Validation(Box::from("invalid payload mime")))?;
    let mut form = reqwest::multipart::Form::new().part("payload_json", payload_part);
    for (i, f) in files.iter().enumerate() {
        common::validate::filename(&f.filename)?;
        let part = reqwest::multipart::Part::bytes(f.bytes.clone())
            .file_name(f.filename.to_string())
            .mime_str(f.content_type.as_str())
            .map_err(|_| common::Error::Validation(Box::from("invalid file mime")))?;
        let mut name_buf = itoa::Buffer::new();
        let idx = name_buf.format(i).to_owned();
        form = form.part(format!("files[{idx}]"), part);
    }
    Ok(form)
}

/// Build a sticker-create form (form-fields variant: `name`, `tags`, `file`).
///
/// # Errors
/// Returns [`common::Error::Validation`] on empty `name`/`tags`, bad
/// filenames, or invalid MIME types.
pub fn sticker_form(
    name: &str,
    tags: &str,
    file: &MultipartFile,
) -> Result<reqwest::multipart::Form, common::Error> {
    if name.is_empty() {
        return Err(common::Error::Validation(Box::from(
            "sticker name must be non-empty",
        )));
    }
    if tags.is_empty() {
        return Err(common::Error::Validation(Box::from(
            "sticker tags must be non-empty",
        )));
    }
    common::validate::filename(&file.filename)?;
    let file_part = reqwest::multipart::Part::bytes(file.bytes.clone())
        .file_name(file.filename.to_string())
        .mime_str(file.content_type.as_str())
        .map_err(|_| common::Error::Validation(Box::from("invalid file mime")))?;
    Ok(reqwest::multipart::Form::new()
        .text("name", name.to_owned())
        .text("tags", tags.to_owned())
        .part("file", file_part))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_json_first_and_filename_validated() {
        let f = MultipartFile::new(
            Box::from("a.png"),
            String::from("image/png"),
            vec![1, 2, 3],
            None,
        );
        assert!(message_form(b"{}", &[f]).is_ok());
        let bad = MultipartFile::new(
            Box::from("a/b.png"),
            String::from("image/png"),
            vec![1],
            None,
        );
        assert!(message_form(b"{}", &[bad]).is_err());
        assert!(message_form(&[0xff, 0xfe], &[]).is_err());
    }

    #[test]
    fn sticker_fields() {
        let f = MultipartFile::new(Box::from("s.png"), String::from("image/png"), vec![1], None);
        assert!(sticker_form("hi", "hi", &f).is_ok());
        assert!(sticker_form("", "hi", &f).is_err());
        assert!(sticker_form("hi", "", &f).is_err());
    }
}
