//! Property-based tests using proptest

use proptest::prelude::*;
use std::path::PathBuf;

#[cfg(test)]
mod path_tests {
    use super::*;

    proptest! {
        #[test]
        fn test_path_creation_doesnt_panic(s in "\\PC*") {
            // Creating a PathBuf from any string should not panic
            let _path = PathBuf::from(&s);
        }

        #[test]
        fn test_path_join_is_associative(
            base in "[a-z]{1,10}",
            mid in "[a-z]{1,10}",
            end in "[a-z]{1,10}"
        ) {
            let path1 = PathBuf::from(&base).join(&mid).join(&end);
            let path2 = PathBuf::from(&base).join(PathBuf::from(&mid).join(&end));

            // Path joining should be associative (somewhat - considering normalization)
            assert_eq!(path1.components().count(), path2.components().count());
        }

        #[test]
        fn test_pathbuf_to_string_roundtrip(s in "[a-zA-Z0-9_/.-]{1,50}") {
            let path = PathBuf::from(&s);
            let as_str = path.to_string_lossy();
            let back = PathBuf::from(as_str.as_ref());

            // Should be able to round-trip through string representation
            assert_eq!(path, back);
        }
    }
}

#[cfg(test)]
mod env_var_tests {
    use super::*;

    proptest! {
        #[test]
        fn test_env_var_names_are_uppercase(s in "[A-Z_][A-Z0-9_]{0,20}") {
            // Valid env var names should work
            let name = s.to_uppercase();
            assert!(name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'));
        }

        #[test]
        fn test_env_var_parsing(
            key in "[A-Z_]{1,20}",
            value in "[a-zA-Z0-9_/.: -]{0,50}"
        ) {
            let env_str = format!("{}={}", key, value);

            // Parse KEY=VALUE format
            if let Some(eq_pos) = env_str.find('=') {
                let (k, v) = env_str.split_at(eq_pos);
                let v = &v[1..]; // Skip '='

                assert_eq!(k, key);
                assert_eq!(v, value);
            }
        }
    }
}

#[cfg(test)]
mod version_tests {
    use super::*;
    use semver::Version;

    proptest! {
        #[test]
        fn test_valid_semver_parsing(
            major in 0u64..100,
            minor in 0u64..100,
            patch in 0u64..100
        ) {
            let version_str = format!("{}.{}.{}", major, minor, patch);
            let version = Version::parse(&version_str);

            assert!(version.is_ok());

            if let Ok(v) = version {
                assert_eq!(v.major, major);
                assert_eq!(v.minor, minor);
                assert_eq!(v.patch, patch);
            }
        }

        #[test]
        fn test_version_comparison_transitive(
            major1 in 0u64..10,
            minor1 in 0u64..10,
            major2 in 0u64..10,
            minor2 in 0u64..10,
            major3 in 0u64..10,
            minor3 in 0u64..10
        ) {
            let v1 = Version::new(major1, minor1, 0);
            let v2 = Version::new(major2, minor2, 0);
            let v3 = Version::new(major3, minor3, 0);

            // If v1 <= v2 and v2 <= v3, then v1 <= v3 (transitivity)
            if v1 <= v2 && v2 <= v3 {
                assert!(v1 <= v3);
            }
        }

        #[test]
        fn test_version_ordering_antisymmetric(
            major1 in 0u64..10,
            minor1 in 0u64..10,
            major2 in 0u64..10,
            minor2 in 0u64..10
        ) {
            let v1 = Version::new(major1, minor1, 0);
            let v2 = Version::new(major2, minor2, 0);

            // If v1 <= v2 and v2 <= v1, then v1 == v2 (antisymmetry)
            if v1 <= v2 && v2 <= v1 {
                assert_eq!(v1, v2);
            }
        }
    }
}

#[cfg(test)]
mod string_tests {
    use super::*;

    proptest! {
        #[test]
        fn test_shlex_split_doesnt_panic(s in "\\PC{0,100}") {
            // Splitting shell commands should not panic
            let _ = shlex::split(&s);
        }

        #[test]
        fn test_simple_command_split_join_roundtrip(
            cmd in "[a-z]{1,10}",
            arg1 in "[a-z0-9]{1,10}",
            arg2 in "[a-z0-9]{1,10}"
        ) {
            let original = format!("{} {} {}", cmd, arg1, arg2);

            if let Some(parts) = shlex::split(&original) {
                assert_eq!(parts.len(), 3);
                assert_eq!(parts[0], cmd);
                assert_eq!(parts[1], arg1);
                assert_eq!(parts[2], arg2);
            }
        }

        #[test]
        fn test_quoted_strings_preserve_spaces(
            content in "[a-z ]{1,20}"
        ) {
            let quoted = format!("\"{}\"", content);

            if let Some(parts) = shlex::split(&quoted) {
                assert_eq!(parts.len(), 1);
                assert_eq!(parts[0], content);
            }
        }
    }
}

#[cfg(test)]
mod regex_tests {
    use super::*;
    use regex::Regex;

    proptest! {
        #[test]
        fn test_simple_glob_to_regex(pattern in "[a-z*?]{1,20}") {
            // Converting glob patterns to regex should not panic
            let regex_pattern = pattern
                .replace(".", r"\.")
                .replace("*", ".*")
                .replace("?", ".");

            let _ = Regex::new(&regex_pattern);
        }

        #[test]
        fn test_regex_match_reflexive(s in "[a-z0-9]{1,20}") {
            // A string should always match itself as a literal regex
            let pattern = regex::escape(&s);
            let re = Regex::new(&pattern).unwrap();

            assert!(re.is_match(&s));
        }
    }
}

#[cfg(test)]
mod collection_tests {
    use super::*;
    use std::collections::HashMap;

    proptest! {
        #[test]
        fn test_hashmap_insert_retrieve(
            pairs in prop::collection::vec(("[a-z]{1,10}", 0i32..100), 1..10)
        ) {
            let mut map = HashMap::new();

            // Insert all pairs
            for (key, value) in &pairs {
                map.insert(key.clone(), *value);
            }

            // Map size should be <= number of pairs (due to duplicate keys)
            assert!(map.len() <= pairs.len());

            // All keys in map should have come from pairs
            for key in map.keys() {
                assert!(pairs.iter().any(|(k, _)| k == key));
            }
        }

        #[test]
        fn test_vec_extend_preserves_order(
            vec1 in prop::collection::vec(0i32..100, 0..10),
            vec2 in prop::collection::vec(0i32..100, 0..10)
        ) {
            let mut combined = vec1.clone();
            combined.extend(vec2.clone());

            // First part should match vec1
            assert_eq!(&combined[..vec1.len()], &vec1[..]);
            // Second part should match vec2
            assert_eq!(&combined[vec1.len()..], &vec2[..]);
        }
    }
}

#[cfg(test)]
mod port_tests {
    use super::*;

    proptest! {
        #[test]
        fn test_port_numbers_in_valid_range(port in 1024u16..65535) {
            // Port numbers in this range should be valid
            assert!(port >= 1024);
            assert!(port < 65535);
        }

        #[test]
        fn test_port_string_parsing(port in 1024u16..65535) {
            let port_str = port.to_string();
            let parsed: u16 = port_str.parse().unwrap();

            assert_eq!(parsed, port);
        }
    }
}
