use std::any::Any;
use std::marker::PhantomData;

use orpc::Schema;
use orpc_procedure::ErasedSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use specta::datatype::DataType;
use specta::{Type, TypeCollection};

/// Non-generic wrapper that stores specta's `DataType` alongside type collection.
///
/// Implements `ErasedSchema` for storage in `ErasedProcedure`. The export engine
/// downcasts to this type via `as_any()` to access the specta type information.
pub struct SpectaSchema {
    pub(crate) data_type: DataType,
    pub(crate) types: TypeCollection,
}

impl ErasedSchema for SpectaSchema {
    fn json_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Typed wrapper that implements the `Schema` trait.
///
/// Use the [`specta()`] function to create instances.
pub struct SpectaSchemaWrapper<T>(PhantomData<T>);

impl<T> SpectaSchemaWrapper<T> {
    pub fn new() -> Self {
        SpectaSchemaWrapper(PhantomData)
    }
}

impl<T> Default for SpectaSchemaWrapper<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Schema for SpectaSchemaWrapper<T>
where
    T: Type + DeserializeOwned + Serialize + Send + Sync + 'static,
{
    type Input = T;
    type Output = T;

    fn validate(&self, input: T) -> Result<T, orpc::ORPCError> {
        Ok(input)
    }

    fn json_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn is_passthrough(&self) -> bool {
        true
    }

    fn into_erased(self) -> Box<dyn ErasedSchema> {
        let mut types = TypeCollection::default();
        let data_type = T::definition(&mut types);
        Box::new(SpectaSchema { data_type, types })
    }
}

/// Create a specta-backed schema for use with the oRPC builder.
///
/// # Example
/// ```ignore
/// use orpc_specta::{specta, Type};
///
/// #[derive(Serialize, Deserialize, Type)]
/// struct Planet { id: u32, name: String }
///
/// let proc = os::<AppCtx>()
///     .input(specta::<FindInput>())
///     .output(specta::<Planet>())
///     .handler(find_planet);
/// ```
pub fn specta<T>() -> SpectaSchemaWrapper<T>
where
    T: Type + DeserializeOwned + Serialize + Send + Sync + 'static,
{
    SpectaSchemaWrapper::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize, Type)]
    struct TestStruct {
        name: String,
        value: u32,
    }

    #[test]
    fn specta_schema_downcast() {
        let wrapper = specta::<TestStruct>();
        let erased: Box<dyn ErasedSchema> = wrapper.into_erased();
        assert!(erased.as_any().downcast_ref::<SpectaSchema>().is_some());
    }

    #[test]
    fn specta_schema_has_data_type() {
        let wrapper = specta::<TestStruct>();
        let erased: Box<dyn ErasedSchema> = wrapper.into_erased();
        let schema = erased.as_any().downcast_ref::<SpectaSchema>().unwrap();
        // Type::definition may return a Reference to a named type or a Struct directly
        assert!(!matches!(schema.data_type, DataType::Nullable(_)));
    }

    #[test]
    fn specta_schema_collects_types() {
        let wrapper = specta::<TestStruct>();
        let erased: Box<dyn ErasedSchema> = wrapper.into_erased();
        let schema = erased.as_any().downcast_ref::<SpectaSchema>().unwrap();
        assert!(!schema.types.is_empty());
    }

    #[test]
    fn specta_is_passthrough() {
        let wrapper = specta::<String>();
        assert!(wrapper.is_passthrough());
    }
}
