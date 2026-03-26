use orpc::ORPCError;

/// Meta type identifiers matching `@orpc/client`'s StandardRPCJsonSerializer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MetaType {
    BigInt = 0,
    Date = 1,
    Nan = 2,
    Undefined = 3,
    Url = 4,
    RegExp = 5,
    Set = 6,
    Map = 7,
}

impl MetaType {
    fn from_u64(v: u64) -> Option<Self> {
        match v {
            0 => Some(MetaType::BigInt),
            1 => Some(MetaType::Date),
            2 => Some(MetaType::Nan),
            3 => Some(MetaType::Undefined),
            4 => Some(MetaType::Url),
            5 => Some(MetaType::RegExp),
            6 => Some(MetaType::Set),
            7 => Some(MetaType::Map),
            _ => None,
        }
    }
}

/// A segment of a JSON path: either an object key or an array index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    Key(String),
    Index(usize),
}

/// A single parsed meta entry: type + path into the JSON value.
#[derive(Debug)]
pub struct MetaEntry {
    pub meta_type: MetaType,
    pub path: Vec<PathSegment>,
}

/// Parse the meta array from the wire format.
///
/// The meta array is always an array of arrays: `[[type_id, ...path], ...]`.
/// Even a single entry is wrapped: `[[1, "createdAt"]]`.
pub fn parse_meta(meta: &[serde_json::Value]) -> Result<Vec<MetaEntry>, ORPCError> {
    if meta.is_empty() {
        return Ok(vec![]);
    }

    meta.iter().map(|entry| {
        let arr = entry.as_array()
            .ok_or_else(|| ORPCError::bad_request("Invalid meta entry: expected array"))?;
        parse_single_entry(arr)
    }).collect()
}

fn parse_single_entry(entry: &[serde_json::Value]) -> Result<MetaEntry, ORPCError> {
    if entry.is_empty() {
        return Err(ORPCError::bad_request("Empty meta entry"));
    }

    let type_id = entry[0].as_u64()
        .ok_or_else(|| ORPCError::bad_request("Meta type must be a number"))?;
    let meta_type = MetaType::from_u64(type_id)
        .ok_or_else(|| ORPCError::bad_request(format!("Unknown meta type: {type_id}")))?;

    let path = entry[1..].iter().map(|seg| {
        if let Some(s) = seg.as_str() {
            Ok(PathSegment::Key(s.to_string()))
        } else if let Some(n) = seg.as_u64() {
            Ok(PathSegment::Index(n as usize))
        } else {
            Err(ORPCError::bad_request("Meta path segment must be string or number"))
        }
    }).collect::<Result<Vec<_>, _>>()?;

    Ok(MetaEntry { meta_type, path })
}

/// Apply meta entries to a JSON value, transforming serialized representations.
///
/// Only types that need action in Rust:
/// - **Undefined (3)**: remove the key from parent object (→ `Option::None` during deserialization)
/// - **Map (7)**: convert `[[k,v],...]` array → `{k:v,...}` object
/// - All others: no-op (BigInt/Date/URL/RegExp as strings, Set as array, NaN as null)
pub fn apply_meta(
    value: &mut serde_json::Value,
    entries: &[MetaEntry],
) -> Result<(), ORPCError> {
    for entry in entries {
        match entry.meta_type {
            MetaType::Undefined => {
                remove_at_path(value, &entry.path)?;
            }
            MetaType::Map => {
                transform_map_at_path(value, &entry.path)?;
            }
            // BigInt, Date, NaN, URL, RegExp, Set — no transformation needed
            _ => {}
        }
    }
    Ok(())
}

/// Navigate to the parent of a path and remove the target key.
fn remove_at_path(
    root: &mut serde_json::Value,
    path: &[PathSegment],
) -> Result<(), ORPCError> {
    if path.is_empty() {
        // Root is undefined — replace with null
        *root = serde_json::Value::Null;
        return Ok(());
    }

    let (parent_path, target) = path.split_at(path.len() - 1);
    let parent = navigate_mut(root, parent_path)?;

    match &target[0] {
        PathSegment::Key(key) => {
            if let Some(obj) = parent.as_object_mut() {
                obj.remove(key);
            }
        }
        PathSegment::Index(idx) => {
            if let Some(arr) = parent.as_array_mut() && *idx < arr.len() {
                arr[*idx] = serde_json::Value::Null;
            }
        }
    }
    Ok(())
}

/// Transform a Map value from `[[k,v],...]` array to `{k:v,...}` object at the given path.
fn transform_map_at_path(
    root: &mut serde_json::Value,
    path: &[PathSegment],
) -> Result<(), ORPCError> {
    let target = navigate_mut(root, path)?;

    if let Some(arr) = target.as_array() {
        let mut map = serde_json::Map::new();
        for pair in arr {
            if let Some(kv) = pair.as_array() && kv.len() == 2 {
                let key = match &kv[0] {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                map.insert(key, kv[1].clone());
            }
        }
        *target = serde_json::Value::Object(map);
    }
    Ok(())
}

/// Navigate to a value at the given path, returning a mutable reference.
fn navigate_mut<'a>(
    root: &'a mut serde_json::Value,
    path: &[PathSegment],
) -> Result<&'a mut serde_json::Value, ORPCError> {
    let mut current = root;
    for segment in path {
        current = match segment {
            PathSegment::Key(key) => current
                .get_mut(key.as_str())
                .ok_or_else(|| ORPCError::bad_request(format!("Meta path not found: {key}")))?,
            PathSegment::Index(idx) => current
                .get_mut(*idx)
                .ok_or_else(|| ORPCError::bad_request(format!("Meta index out of bounds: {idx}")))?,
        };
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_empty_meta() {
        let entries = parse_meta(&[]).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_single_entry() {
        // TS always wraps single entries: [[type, ...path]]
        let meta = vec![json!([1, "createdAt"])];
        let entries = parse_meta(&meta).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].meta_type, MetaType::Date);
        assert_eq!(entries[0].path, vec![PathSegment::Key("createdAt".into())]);
    }

    #[test]
    fn parse_single_entry_nested_path() {
        let meta = vec![json!([0, "data", "count"])];
        let entries = parse_meta(&meta).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].meta_type, MetaType::BigInt);
        assert_eq!(entries[0].path, vec![
            PathSegment::Key("data".into()),
            PathSegment::Key("count".into()),
        ]);
    }

    #[test]
    fn parse_multiple_entries() {
        let meta = vec![
            json!([0, "count"]),
            json!([1, "updated"]),
        ];
        let entries = parse_meta(&meta).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].meta_type, MetaType::BigInt);
        assert_eq!(entries[1].meta_type, MetaType::Date);
    }

    #[test]
    fn parse_with_array_index() {
        let meta = vec![json!([1, "items", 0, "date"])];
        let entries = parse_meta(&meta).unwrap();
        assert_eq!(entries[0].path, vec![
            PathSegment::Key("items".into()),
            PathSegment::Index(0),
            PathSegment::Key("date".into()),
        ]);
    }

    #[test]
    fn parse_invalid_type() {
        let meta = vec![json!([99])];
        assert!(parse_meta(&meta).is_err());
    }

    #[test]
    fn parse_entry_without_path() {
        // Type ID only, no path — applies to root
        let meta = vec![json!([6])];
        let entries = parse_meta(&meta).unwrap();
        assert_eq!(entries[0].meta_type, MetaType::Set);
        assert!(entries[0].path.is_empty());
    }

    #[test]
    fn apply_undefined_removes_key() {
        let mut value = json!({"name": "Alice", "deleted": null});
        let entries = vec![MetaEntry {
            meta_type: MetaType::Undefined,
            path: vec![PathSegment::Key("deleted".into())],
        }];
        apply_meta(&mut value, &entries).unwrap();
        assert!(value.get("deleted").is_none());
        assert_eq!(value.get("name").unwrap(), "Alice");
    }

    #[test]
    fn apply_map_transforms_array_to_object() {
        let mut value = json!({"data": [["a", 1], ["b", 2]]});
        let entries = vec![MetaEntry {
            meta_type: MetaType::Map,
            path: vec![PathSegment::Key("data".into())],
        }];
        apply_meta(&mut value, &entries).unwrap();
        assert_eq!(value["data"]["a"], 1);
        assert_eq!(value["data"]["b"], 2);
    }

    #[test]
    fn apply_noop_types() {
        let mut value = json!({"count": "12345678901234567890", "date": "2024-01-01T00:00:00Z"});
        let original = value.clone();
        let entries = vec![
            MetaEntry { meta_type: MetaType::BigInt, path: vec![PathSegment::Key("count".into())] },
            MetaEntry { meta_type: MetaType::Date, path: vec![PathSegment::Key("date".into())] },
        ];
        apply_meta(&mut value, &entries).unwrap();
        assert_eq!(value, original);
    }

    #[test]
    fn apply_undefined_at_root() {
        let mut value = json!(null);
        let entries = vec![MetaEntry {
            meta_type: MetaType::Undefined,
            path: vec![],
        }];
        apply_meta(&mut value, &entries).unwrap();
        assert_eq!(value, json!(null));
    }

    #[test]
    fn apply_nested_map() {
        let mut value = json!({"response": {"mapping": [["x", 10], ["y", 20]]}});
        let entries = vec![MetaEntry {
            meta_type: MetaType::Map,
            path: vec![PathSegment::Key("response".into()), PathSegment::Key("mapping".into())],
        }];
        apply_meta(&mut value, &entries).unwrap();
        assert_eq!(value["response"]["mapping"]["x"], 10);
        assert_eq!(value["response"]["mapping"]["y"], 20);
    }
}
