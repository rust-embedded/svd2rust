#![cfg(feature = "serde")]

use crate::util::SourceType;
use crate::util::{Target, SOURCE_TYPE_NAMES, TARGET_NAMES};
use core::fmt;
use core::str;
use serde::{
    de::{DeserializeSeed, EnumAccess, Error, Unexpected, VariantAccess, Visitor},
    Deserialize, Deserializer,
};

impl<'de> Deserialize<'de> for Target {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TargetIdentifier;

        impl<'de> Visitor<'de> for TargetIdentifier {
            type Value = Target;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("svd2rust target")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Target::parse(s).map_err(|_| Error::unknown_variant(s, &TARGET_NAMES))
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
            where
                E: Error,
            {
                let variant = str::from_utf8(value)
                    .map_err(|_| Error::invalid_value(Unexpected::Bytes(value), &self))?;

                self.visit_str(variant)
            }
        }

        impl<'de> DeserializeSeed<'de> for TargetIdentifier {
            type Value = Target;

            fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer.deserialize_identifier(TargetIdentifier)
            }
        }

        struct TargetEnum;

        impl<'de> Visitor<'de> for TargetEnum {
            type Value = Target;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("svd2rust target")
            }

            fn visit_enum<A>(self, value: A) -> Result<Self::Value, A::Error>
            where
                A: EnumAccess<'de>,
            {
                let (ans, variant) = value.variant_seed(TargetIdentifier)?;
                // Every variant is a unit variant.
                variant.unit_variant()?;
                Ok(ans)
            }
        }

        deserializer.deserialize_enum("Target", &TARGET_NAMES, TargetEnum)
    }
}

impl<'de> Deserialize<'de> for SourceType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SourceTypeIdentifier;

        impl<'de> Visitor<'de> for SourceTypeIdentifier {
            type Value = SourceType;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("svd2rust source type")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                SourceType::from_extension(s).ok_or(Error::unknown_variant(s, &SOURCE_TYPE_NAMES))
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
            where
                E: Error,
            {
                let variant = str::from_utf8(value)
                    .map_err(|_| Error::invalid_value(Unexpected::Bytes(value), &self))?;

                self.visit_str(variant)
            }
        }

        impl<'de> DeserializeSeed<'de> for SourceTypeIdentifier {
            type Value = SourceType;

            fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer.deserialize_identifier(SourceTypeIdentifier)
            }
        }

        struct SourceTypeEnum;

        impl<'de> Visitor<'de> for SourceTypeEnum {
            type Value = SourceType;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("svd2rust source type")
            }

            fn visit_enum<A>(self, value: A) -> Result<Self::Value, A::Error>
            where
                A: EnumAccess<'de>,
            {
                let (ans, variant) = value.variant_seed(SourceTypeIdentifier)?;
                // Every variant is a unit variant.
                variant.unit_variant()?;
                Ok(ans)
            }
        }

        deserializer.deserialize_enum("SourceType", &SOURCE_TYPE_NAMES, SourceTypeEnum)
    }
}
