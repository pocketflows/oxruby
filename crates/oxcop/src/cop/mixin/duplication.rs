//! Port of `lib/rubocop/cop/mixin/duplication.rb`. Elements are compared
//! with `eq` (Ruby's `==`/`eql?`; `Node::same_as` for nodes).

/// `duplicates?(collection)`: whether the collection contains duplicates.
pub fn has_duplicates<T: Copy>(collection: &[T], eq: impl Fn(T, T) -> bool) -> bool {
    collection.len() > 1
        && grouped_duplicates(collection, eq)
            .iter()
            .any(|g| g.len() > 1)
}

/// `duplicates(collection)`: all duplicates, including the first instance of
/// each duplicated element.
pub fn duplicates<T: Copy>(collection: &[T], eq: impl Fn(T, T) -> bool) -> Vec<T> {
    grouped_duplicates(collection, eq)
        .into_iter()
        .flatten()
        .collect()
}

/// `consecutive_duplicates(collection)`: the duplicates, leaving out the
/// first instance of each duplicated element.
pub fn consecutive_duplicates<T: Copy>(collection: &[T], eq: impl Fn(T, T) -> bool) -> Vec<T> {
    grouped_duplicates(collection, eq)
        .into_iter()
        .flat_map(|items| items.into_iter().skip(1))
        .collect()
}

/// `grouped_duplicates(collection)`: the groups of equal elements (in order
/// of first appearance) that have more than one element.
pub fn grouped_duplicates<T: Copy>(collection: &[T], eq: impl Fn(T, T) -> bool) -> Vec<Vec<T>> {
    let mut groups: Vec<Vec<T>> = Vec::new();
    for &item in collection {
        match groups.iter_mut().find(|group| eq(group[0], item)) {
            Some(group) => group.push(item),
            None => groups.push(vec![item]),
        }
    }
    groups.retain(|group| group.len() > 1);
    groups
}
