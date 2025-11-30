//! Asset handling utilities (template files, bundled defaults).

/// Get embedded asset bytes by path.
/// Returns None if the asset path is not recognized.
pub fn get_asset(path: &str) -> Option<&'static [u8]> {
    match path {
        "php/php.ini" => Some(include_bytes!("../assets/php/php.ini")),
        "wordpress/install.sh" => Some(include_bytes!("../assets/wordpress/install.sh")),
        "wordpress/wp-config.php" => Some(include_bytes!("../assets/wordpress/wp-config.php")),
        _ => None,
    }
}
