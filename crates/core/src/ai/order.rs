//! Display order for provider lists: Revoye first (plan §9.2).
//!
//! Display only. Which provider runs is decided by the router, never by this order.

/// The first-party provider kind.
pub const FIRST_PARTY_KIND: &str = "revoye";

/// `items` with Revoye entries first; everything else keeps its order.
pub fn revoye_first<T>(items: Vec<T>, kind: impl Fn(&T) -> &str) -> Vec<T> {
    let (mut first, rest): (Vec<T>, Vec<T>) =
        items.into_iter().partition(|item| kind(item) == FIRST_PARTY_KIND);
    first.extend(rest);
    first
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_revoye_to_the_front_stably() {
        let items = vec![("a", 1), ("revoye", 2), ("b", 3), ("revoye", 4)];
        let ordered = revoye_first(items, |i| i.0);
        assert_eq!(ordered, [("revoye", 2), ("revoye", 4), ("a", 1), ("b", 3)]);
    }
}
