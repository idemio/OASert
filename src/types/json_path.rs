use crate::{ENCODED_BACKSLASH, ENCODED_TILDE, PATH_SEPARATOR, TILDE};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct JsonPath(pub Vec<String>);

impl JsonPath {
    pub fn new() -> Self {
        JsonPath(Vec::new())
    }

    pub fn add(&mut self, segment: impl AsRef<str>) -> &mut Self {
        let segment = segment.as_ref();
        if segment.contains(TILDE) || segment.contains(PATH_SEPARATOR) {
            let segment = segment
                .replace(TILDE, ENCODED_TILDE)
                .replace(PATH_SEPARATOR, ENCODED_BACKSLASH);
            self.0.push(segment);
        } else {
            self.0.push(segment.to_owned());
        }

        self
    }

    pub fn format_path(&self) -> String {
        self.0.join(PATH_SEPARATOR)
    }
}

#[cfg(test)]
mod test {
    use crate::types::json_path::JsonPath;
    use crate::{ENCODED_BACKSLASH, ENCODED_TILDE, PATH_SEPARATOR};

    #[test]
    fn test_new_json_path() {
        let path = JsonPath::new();
        assert_eq!(path.0.len(), 0);
        assert_eq!(path.format_path(), "");
    }

    #[test]
    fn test_add_simple_segment() {
        let mut path = JsonPath::new();
        path.add("simple");
        assert_eq!(path.0.len(), 1);
        assert_eq!(path.0[0], "simple");
        assert_eq!(path.format_path(), "simple");
    }

    #[test]
    fn test_add_multiple_segments() {
        let mut path = JsonPath::new();
        path.add("component").add("schemas").add("User");
        assert_eq!(path.0.len(), 3);
        assert_eq!(path.0[0], "component");
        assert_eq!(path.0[1], "schemas");
        assert_eq!(path.0[2], "User");
        assert_eq!(path.format_path(), "component/schemas/User");
    }

    #[test]
    fn test_add_segment_with_tilde() {
        let mut path = JsonPath::new();
        path.add("user~name");
        assert_eq!(path.0.len(), 1);
        assert_eq!(path.0[0], format!("user{}name", ENCODED_TILDE));

        // Check that the tilde is properly encoded in the formatted path
        let formatted = path.format_path();
        assert_eq!(formatted, format!("user{}name", ENCODED_TILDE));
    }

    #[test]
    fn test_add_segment_with_slash() {
        let mut path = JsonPath::new();
        path.add("user/profile");
        assert_eq!(path.0.len(), 1);
        assert_eq!(path.0[0], format!("user{}profile", ENCODED_BACKSLASH));

        // Check that the slash is properly encoded in the formatted path
        let formatted = path.format_path();
        assert_eq!(formatted, format!("user{}profile", ENCODED_BACKSLASH));
        assert!(!formatted.contains("/"));
    }

    #[test]
    fn test_add_segment_with_tilde_and_slash() {
        let mut path = JsonPath::new();
        path.add("user~/profile");
        assert_eq!(path.0.len(), 1);

        // Both special characters should be encoded
        let expected = "user".to_string() + ENCODED_TILDE + ENCODED_BACKSLASH + "profile";
        assert_eq!(path.0[0], expected);

        // Check that both special characters are properly encoded in the formatted path
        let formatted = path.format_path();
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_format_path_empty() {
        let path = JsonPath::new();
        assert_eq!(path.format_path(), "");
    }

    #[test]
    fn test_format_path_single_segment() {
        let mut path = JsonPath::new();
        path.add("test");
        assert_eq!(path.format_path(), "test");
    }

    #[test]
    fn test_format_path_complex() {
        let mut path = JsonPath::new();
        path.add("paths")
            .add("/users/{id}")
            .add("get")
            .add("responses")
            .add("200");

        // The segment with slashes should be encoded
        let expected_second = format!("{}users{}{{id}}", ENCODED_BACKSLASH, ENCODED_BACKSLASH);
        assert_eq!(path.0[1], expected_second);

        // The formatted path should have the proper separators and encodings
        let expected_path = format!(
            "paths{}{}{}{}{}{}{}{}",
            PATH_SEPARATOR,
            expected_second,
            PATH_SEPARATOR,
            "get",
            PATH_SEPARATOR,
            "responses",
            PATH_SEPARATOR,
            "200"
        );
        assert_eq!(path.format_path(), expected_path);
    }

    #[test]
    fn test_chained_add_operations() {
        let mut path = JsonPath::new();
        let result = path.add("first").add("second").add("third");

        // Verify that we get a mutable reference back each time for chaining
        assert_eq!(result.0.len(), 3);
        assert_eq!(path.0.len(), 3);
        assert_eq!(path.format_path(), "first/second/third");
    }

    #[test]
    fn test_add_empty_segment() {
        let mut path = JsonPath::new();
        path.add("");
        assert_eq!(path.0.len(), 1);
        assert_eq!(path.0[0], "");
        assert_eq!(path.format_path(), "");
    }

    #[test]
    fn test_json_pointer_compatibility() {
        // Test that the path representation is compatible with JSON Pointer format
        // by creating paths that would be used in real OpenAPI specs

        let mut path = JsonPath::new();
        path.add("components").add("schemas").add("Error");
        assert_eq!(path.format_path(), "components/schemas/Error");

        let mut path = JsonPath::new();
        path.add("paths")
            .add("/pets")
            .add("get")
            .add("parameters")
            .add("0");

        // The '/pets' segment should have its slash encoded
        let expected_second = format!("{}pets", ENCODED_BACKSLASH);
        assert_eq!(path.0[1], expected_second);

        let expected_path = format!(
            "paths{}{}{}{}{}{}{}{}",
            PATH_SEPARATOR,
            expected_second,
            PATH_SEPARATOR,
            "get",
            PATH_SEPARATOR,
            "parameters",
            PATH_SEPARATOR,
            "0"
        );
        assert_eq!(path.format_path(), expected_path);
    }

    #[test]
    fn test_special_characters_encoding() {
        let mut path = JsonPath::new();

        // Test various special character combinations
        path.add("a~b/c");
        path.add("d/e~f");
        path.add("~~/~~");
        path.add("//");

        assert_eq!(path.0.len(), 4);

        // First segment: a~b/c -> a~0b~1c
        assert_eq!(
            path.0[0],
            format!("a{}b{}c", ENCODED_TILDE, ENCODED_BACKSLASH)
        );

        // Second segment: d/e~f -> d~1e~0f
        assert_eq!(
            path.0[1],
            format!("d{}e{}f", ENCODED_BACKSLASH, ENCODED_TILDE)
        );

        // Third segment: ~~/~~ -> ~0~0~1~0~0
        assert_eq!(
            path.0[2],
            format!("{0}{0}{1}{0}{0}", ENCODED_TILDE, ENCODED_BACKSLASH)
        );

        // Fourth segment: // -> ~1~1
        assert_eq!(path.0[3], format!("{0}{0}", ENCODED_BACKSLASH));

        // Verify the entire formatted path
        let expected_path = [
            format!("a{}b{}c", ENCODED_TILDE, ENCODED_BACKSLASH),
            format!("d{}e{}f", ENCODED_BACKSLASH, ENCODED_TILDE),
            format!("{0}{0}{1}{0}{0}", ENCODED_TILDE, ENCODED_BACKSLASH),
            format!("{0}{0}", ENCODED_BACKSLASH),
        ]
        .join(PATH_SEPARATOR);

        assert_eq!(path.format_path(), expected_path);
    }

    #[test]
    fn test_numeric_segments() {
        let mut path = JsonPath::new();
        path.add("items").add("0").add("name");

        assert_eq!(path.0.len(), 3);
        assert_eq!(path.0[0], "items");
        assert_eq!(path.0[1], "0");
        assert_eq!(path.0[2], "name");
        assert_eq!(path.format_path(), "items/0/name");
    }

    #[test]
    fn test_modify_existing_path() {
        let mut path = JsonPath::new();
        path.add("components").add("schemas");

        assert_eq!(path.format_path(), "components/schemas");

        // Now modify by adding more segments
        path.add("User").add("properties").add("email");

        assert_eq!(path.0.len(), 5);
        assert_eq!(
            path.format_path(),
            "components/schemas/User/properties/email"
        );
    }
}
