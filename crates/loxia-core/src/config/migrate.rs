//! schema_version migrations.
//!
//! Runs on the raw `toml::Value` tree **before** deserialization into `Config`. Schema version 1
//! is current, so today this is a no-op that stamps `schema_version = 1` when the key is absent.
//! It exists now so the first breaking change has somewhere to go — a future migration adds a
//! match arm here, not a rewrite of this module's shape.

use super::{ConfigWarning, Severity};

pub const CURRENT_SCHEMA_VERSION: i64 = 1;

/// Reads (and, if absent, stamps) `schema_version` on the raw table. A version **greater** than
/// the current one warns loudly ("written by a newer loxia") and proceeds unchanged — downgrading
/// must never wipe a user's settings, and a version *older* than current would be handled by an
/// actual migration step here once one exists.
pub fn migrate(raw: &mut toml::Value) -> Vec<ConfigWarning> {
    let mut warnings = Vec::new();

    let Some(table) = raw.as_table_mut() else {
        return warnings;
    };

    let version = match table.get("schema_version").and_then(|v| v.as_integer()) {
        Some(v) => v,
        None => {
            table.insert(
                "schema_version".to_string(),
                toml::Value::Integer(CURRENT_SCHEMA_VERSION),
            );
            CURRENT_SCHEMA_VERSION
        }
    };

    if version > CURRENT_SCHEMA_VERSION {
        warnings.push(ConfigWarning {
            field: "schema_version".to_string(),
            message: format!(
                "config was written by a newer loxia (schema {version}, this build understands \
                 {CURRENT_SCHEMA_VERSION}); some settings may be ignored"
            ),
            severity: Severity::Warning,
        });
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_schema_version_is_stamped() {
        let mut raw: toml::Value = toml::from_str("").unwrap();
        let warnings = migrate(&mut raw);
        assert!(warnings.is_empty());
        assert_eq!(
            raw.as_table()
                .unwrap()
                .get("schema_version")
                .unwrap()
                .as_integer(),
            Some(CURRENT_SCHEMA_VERSION)
        );
    }

    #[test]
    fn current_version_is_a_noop() {
        let mut raw: toml::Value = toml::from_str("schema_version = 1").unwrap();
        let warnings = migrate(&mut raw);
        assert!(warnings.is_empty());
    }

    #[test]
    fn future_version_warns_and_proceeds() {
        let mut raw: toml::Value = toml::from_str("schema_version = 999").unwrap();
        let warnings = migrate(&mut raw);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].field, "schema_version");
        assert_eq!(
            raw.as_table()
                .unwrap()
                .get("schema_version")
                .unwrap()
                .as_integer(),
            Some(999)
        );
    }
}
