pub mod href;
pub mod mp4;
pub mod mp4_atom_properties;
pub mod network;
pub mod query_codec;
pub mod response;

#[cfg(test)]
mod tests {
    // Because we use a HashMap as the input when decoding to the query string value, the order of
    // parameters is non-deterministic, so this method helps validate the string is as expected.
    pub fn assert_definitions_string_equality(expected: &str, actual: &str) {
        let expected_vec = expected.split("%22").fold(Vec::new(), |v, s| {
            let mut vec = vec![s];
            vec.extend(v);
            vec
        });
        let actual_vec = actual.split("%22").fold(Vec::new(), |v, s| {
            let mut vec = vec![s];
            vec.extend(v);
            vec
        });
        for expected in &expected_vec {
            assert!(actual_vec.contains(expected));
        }
        for actual in &actual_vec {
            assert!(expected_vec.contains(actual));
        }
    }
}
