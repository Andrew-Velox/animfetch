//! A heading and the rows that hang under it, as the tree and sections styles
//! both draw them.

use crate::fetch::Item;
use crate::render::icons::{self, Glyph};

/// What a heading says when nothing could be found to put there.
pub const UNKNOWN: &str = "unknown";

/// A group of rows under one heading. The heading takes the first of `head`
/// that is present, so a machine with no usable host name still has something
/// to call itself.
pub struct Group {
    pub name: &'static str,
    pub head: &'static [&'static str],
    pub children: &'static [&'static str],
}

pub struct Members<'a> {
    pub head: Option<&'a Item>,
    pub leaves: Vec<(&'a Item, &'static Glyph)>,
}

impl Group {
    /// The icon beside the heading: that of the row it is usually named by.
    pub fn icon(&self) -> &'static str {
        self.head
            .first()
            .map_or(icons::GENERIC.icon, |l| icons::glyph(l).icon)
    }

    /// The rows present for this group, or `None` if the machine had none.
    pub fn members<'a>(&self, items: &'a [Item]) -> Option<Members<'a>> {
        let head = self
            .head
            .iter()
            .find_map(|l| items.iter().find(|i| i.label == *l));

        // A row that headed the group is not also one of its leaves.
        let leaves: Vec<_> = self
            .children
            .iter()
            .flat_map(|&label| {
                items
                    .iter()
                    .filter(move |i| i.label == label)
                    .map(move |i| (i, icons::glyph(label)))
            })
            .filter(|(i, _)| !head.is_some_and(|h| std::ptr::eq(h, *i)))
            .collect();

        (head.is_some() || !leaves.is_empty()).then_some(Members { head, leaves })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PC: Group = Group {
        name: "PC",
        head: &["Host", "CPU"],
        children: &["CPU", "GPU"],
    };

    fn items(labels: &[&'static str]) -> Vec<Item> {
        labels.iter().map(|l| Item::new(l, *l)).collect()
    }

    #[test]
    fn the_first_present_head_names_the_group() {
        let rows = items(&["CPU", "Host", "GPU"]);
        let m = PC.members(&rows).unwrap();
        assert_eq!(m.head.unwrap().label, "Host");
    }

    #[test]
    fn a_missing_head_is_replaced_by_the_next_and_is_not_also_a_leaf() {
        let rows = items(&["CPU", "GPU"]);
        let m = PC.members(&rows).unwrap();

        assert_eq!(m.head.unwrap().label, "CPU");
        let leaves: Vec<_> = m.leaves.iter().map(|(i, _)| i.label).collect();
        assert_eq!(leaves, ["GPU"], "the heading must not repeat as a leaf");
    }

    #[test]
    fn a_group_with_nothing_is_absent() {
        assert!(PC.members(&items(&["Kernel"])).is_none());
        assert!(PC.members(&[]).is_none());
    }

    #[test]
    fn leaves_without_a_head_still_make_a_group() {
        let rows = items(&["GPU"]);
        let m = PC.members(&rows).unwrap();
        assert!(m.head.is_none());
        assert_eq!(m.leaves.len(), 1);
    }

    #[test]
    fn repeated_rows_each_get_a_leaf() {
        // Two disks, two batteries: every one is shown, in order.
        let rows = vec![Item::new("GPU", "a"), Item::new("GPU", "b")];
        let m = PC.members(&rows).unwrap();
        let values: Vec<_> = m.leaves.iter().map(|(i, _)| i.value.as_str()).collect();
        assert_eq!(values, ["a", "b"]);
    }

    #[test]
    fn the_icon_is_the_one_the_head_is_known_by() {
        assert_eq!(PC.icon(), icons::glyph("Host").icon);
    }
}
