use std::collections::HashMap;

fn query_to_prefix(query: String) -> String {
    query
        .find('*')
        .map(|index| query[0..index].to_string())
        .unwrap_or(query)
}

pub fn wildcard_get_all<T>(hashmap: &HashMap<String, T>, query: String) -> Vec<T>
where
    T: Clone,
{
    let query = query_to_prefix(query);

    hashmap
        .iter()
        .filter(|(k, _)| {
            let key_query = query_to_prefix(k.to_string());
            k.starts_with(query.as_str()) || query.starts_with(key_query.as_str())
        })
        .map(|(_, v)| v.clone())
        .collect()
}

pub fn wildcard_get<T>(hashmap: &HashMap<String, T>, query: String) -> Option<T>
where
    T: Clone,
{
    wildcard_get_all(hashmap, query).first().cloned()
}
