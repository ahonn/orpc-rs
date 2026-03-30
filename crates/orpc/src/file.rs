use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

/// In-memory representation of an uploaded file.
///
/// Serde-compatible: serializes as `{"data": "<base64>", "name": "...", "contentType": "..."}`.
/// Used as a field type in handler input structs to receive file uploads:
///
/// ```ignore
/// #[derive(Deserialize)]
/// struct UploadInput {
///     title: String,
///     avatar: ORPCFile,
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ORPCFile {
    /// Raw file bytes.
    pub data: Vec<u8>,
    /// Original filename, if provided.
    pub name: Option<String>,
    /// MIME type, if provided (e.g. `"image/png"`).
    pub content_type: Option<String>,
}

impl ORPCFile {
    pub fn new(data: Vec<u8>) -> Self {
        ORPCFile {
            data,
            name: None,
            content_type: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }
}

impl Serialize for ORPCFile {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("ORPCFile", 3)?;
        state.serialize_field("data", &BASE64.encode(&self.data))?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("contentType", &self.content_type)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ORPCFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "camelCase")]
        enum Field {
            Data,
            Name,
            ContentType,
        }

        struct ORPCFileVisitor;

        impl<'de> Visitor<'de> for ORPCFileVisitor {
            type Value = ORPCFile;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an ORPCFile object with base64-encoded data")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<ORPCFile, M::Error> {
                let mut data: Option<String> = None;
                let mut name: Option<String> = None;
                let mut content_type: Option<String> = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Data => data = Some(map.next_value()?),
                        Field::Name => name = map.next_value()?,
                        Field::ContentType => content_type = map.next_value()?,
                    }
                }

                let data_str = data.ok_or_else(|| de::Error::missing_field("data"))?;
                let bytes = BASE64
                    .decode(&data_str)
                    .map_err(|e| de::Error::custom(format!("invalid base64 data: {e}")))?;

                Ok(ORPCFile {
                    data: bytes,
                    name,
                    content_type,
                })
            }
        }

        deserializer.deserialize_struct(
            "ORPCFile",
            &["data", "name", "contentType"],
            ORPCFileVisitor,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_roundtrip() {
        let file = ORPCFile::new(b"hello world".to_vec())
            .with_name("test.txt")
            .with_content_type("text/plain");

        let json = serde_json::to_value(&file).unwrap();
        assert_eq!(json["data"], BASE64.encode(b"hello world"));
        assert_eq!(json["name"], "test.txt");
        assert_eq!(json["contentType"], "text/plain");

        let deserialized: ORPCFile = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.data, b"hello world");
        assert_eq!(deserialized.name.as_deref(), Some("test.txt"));
        assert_eq!(deserialized.content_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn deserialize_minimal() {
        let json = serde_json::json!({
            "data": BASE64.encode(b"bytes"),
        });
        let file: ORPCFile = serde_json::from_value(json).unwrap();
        assert_eq!(file.data, b"bytes");
        assert!(file.name.is_none());
        assert!(file.content_type.is_none());
    }

    #[test]
    fn deserialize_invalid_base64() {
        let json = serde_json::json!({
            "data": "not valid base64!!!",
        });
        let result = serde_json::from_value::<ORPCFile>(json);
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_missing_data() {
        let json = serde_json::json!({
            "name": "test.txt",
        });
        let result = serde_json::from_value::<ORPCFile>(json);
        assert!(result.is_err());
    }

    #[test]
    fn empty_file() {
        let file = ORPCFile::new(vec![]);
        let json = serde_json::to_value(&file).unwrap();
        let deserialized: ORPCFile = serde_json::from_value(json).unwrap();
        assert!(deserialized.data.is_empty());
    }
}
