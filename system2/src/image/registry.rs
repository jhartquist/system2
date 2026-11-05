// each image whose reference can be in a request, so `ask` can send its bytes

use super::{Bytes, InvalidImage, Shared};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

static IMAGES: Mutex<BTreeMap<String, Entry>> = Mutex::new(BTreeMap::new());

enum Entry {
    Static(&'static [u8]),
    Shared(Weak<Shared>),
}

// two equal images share one entry, so either one keeps it
pub(super) fn register(shared: Shared) -> Result<Arc<Shared>, InvalidImage> {
    let mut images = lock_images();
    let Some(found) = find_in(&images, &shared.reference) else {
        let shared = Arc::new(shared);
        let entry = Entry::Shared(Arc::downgrade(&shared));
        images.insert(shared.reference.clone(), entry);
        return Ok(shared);
    };

    // the drop of the last `Arc` takes the lock, so each drop comes after the unlock
    drop(images);
    if found.as_slice() != shared.bytes {
        let reference = shared.reference.clone();
        return Err(InvalidImage::Reference { reference });
    }
    match found {
        Bytes::Shared(found) => Ok(found),

        // the entry of a static file stays, so this copy needs no entry
        Bytes::Static(_) => Ok(Arc::new(shared)),
    }
}

// a file of `image!` lives as long as the program, so its entry stays
pub(super) fn register_static(reference: &str, bytes: &'static [u8]) {
    lock_images().insert(reference.to_owned(), Entry::Static(bytes));
}

// the entry stays if a static file or a newer image holds it
pub(super) fn forget(reference: &str) {
    let mut images = lock_images();
    if let Some(Entry::Shared(weak)) = images.get(reference)
        && weak.strong_count() == 0
    {
        images.remove(reference);
    }
}

pub(crate) fn find(reference: &str) -> Option<Bytes> {
    find_in(&lock_images(), reference)
}

fn find_in(images: &BTreeMap<String, Entry>, reference: &str) -> Option<Bytes> {
    match images.get(reference)? {
        Entry::Static(bytes) => Some(Bytes::Static(bytes)),
        Entry::Shared(weak) => weak.upgrade().map(Bytes::Shared),
    }
}

fn lock_images() -> MutexGuard<'static, BTreeMap<String, Entry>> {
    IMAGES.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::super::{Image, InvalidImage, Shared, reference_of};
    use super::{Entry, find, lock_images, register_static};
    use std::sync::Arc;

    // a byte past the end of the file, so that no other test makes the same image
    fn red(extra_byte: u8) -> Vec<u8> {
        let mut bytes = include_bytes!("../../tests/images/red.png").to_vec();
        bytes.push(extra_byte);
        bytes
    }

    #[test]
    fn register_shares_one_entry_between_equal_images() {
        let reference = reference_of(&red(1));
        let first = Image::new(red(1)).unwrap();
        let second = Image::new(red(1)).unwrap();

        drop(first);
        assert!(find(&reference).is_some());
        drop(second);
        assert!(!lock_images().contains_key(&reference));
    }

    #[test]
    fn register_refuses_a_different_image_with_the_same_reference() {
        let reference = reference_of(&red(2));
        let reference_of_other = reference.clone();
        let other = Arc::new(Shared {
            reference: reference_of_other,
            bytes: b"other bytes".to_vec(),
        });
        lock_images().insert(reference.clone(), Entry::Shared(Arc::downgrade(&other)));

        let error = Image::new(red(2)).unwrap_err();

        assert!(
            matches!(&error, InvalidImage::Reference { reference: found } if *found == reference)
        );
        assert_eq!(
            error.to_string(),
            format!(
                "each image must have a different reference, but a different image has the \
                 reference `{reference}`: change a byte of one of them"
            )
        );
        drop(other);
        assert!(find(&reference).is_none());
    }

    #[test]
    fn register_static_keeps_its_entry_after_a_copy_drops() {
        static BYTES: &[u8] = &[0x89, b'P', b'N', b'G', 3];
        let reference = reference_of(BYTES);
        register_static(&reference, BYTES);

        let copy = Arc::new(Shared {
            reference: reference.clone(),
            bytes: BYTES.to_vec(),
        });
        drop(copy);

        assert_eq!(find(&reference).unwrap().as_slice(), BYTES);
    }

    #[test]
    fn register_gives_a_copy_of_a_static_image_its_own_bytes() {
        let bytes: &'static [u8] = red(3).leak();
        let reference = reference_of(bytes);
        register_static(&reference, bytes);

        let copy = Image::new(red(3)).unwrap();
        drop(copy);

        assert_eq!(find(&reference).unwrap().as_slice(), bytes);
    }
}
