// the options or the levels of a question: rust cannot make a `Vec` with items in a const, so a
// macro gives a function that makes them at the first use, and a constructor gives them owned
#[derive(Debug, Clone)]
pub(crate) enum Table<M, E> {
    Deferred(fn() -> Vec<M>),
    Owned(Vec<E>),
}

impl<M, E: From<M>> Table<M, E> {
    // the table keeps the entries that it makes, so the values that a question sends are the
    // values of its answer
    pub(crate) fn entries(&mut self) -> &[E] {
        if let Table::Deferred(macro_entries) = *self {
            *self = Table::Owned(entries_of(macro_entries));
        }
        self.as_entries()
    }

    pub(crate) fn into_entries(self) -> Vec<E> {
        match self {
            Table::Deferred(macro_entries) => entries_of(macro_entries),
            Table::Owned(entries) => entries,
        }
    }

    // `write` and `read` make a table owned with `entries`, so an answer lends an owned table
    pub(crate) fn as_entries(&self) -> &[E] {
        match self {
            Table::Owned(entries) => entries,
            Table::Deferred(_) => unreachable!("a table is owned after its first use"),
        }
    }
}

fn entries_of<M, E: From<M>>(macro_entries: fn() -> Vec<M>) -> Vec<E> {
    macro_entries().into_iter().map(E::from).collect()
}

#[cfg(test)]
mod tests {
    use super::Table;

    #[test]
    fn into_entries_gives_the_entries_of_each_table() {
        let tables: [Table<u8, u32>; 2] =
            [Table::Deferred(|| vec![1, 2]), Table::Owned(vec![1, 2])];

        for table in tables {
            assert_eq!(table.into_entries(), [1, 2]);
        }
    }

    #[test]
    fn entries_keeps_the_entries_of_a_deferred_table() {
        let mut table: Table<u8, u32> = Table::Deferred(|| vec![1, 2]);

        let made_entries = table.entries().as_ptr();
        let kept_entries = table.entries();
        assert_eq!(kept_entries, [1, 2]);
        assert_eq!(kept_entries.as_ptr(), made_entries);
    }

    #[test]
    fn as_entries_lends_an_owned_table() {
        let table: Table<u8, u32> = Table::Owned(vec![1, 2]);

        assert_eq!(table.as_entries(), [1, 2]);
    }

    #[test]
    #[should_panic(expected = "a table is owned after its first use")]
    fn as_entries_refuses_a_deferred_table() {
        let table: Table<u8, u32> = Table::Deferred(Vec::new);

        _ = table.as_entries();
    }
}
