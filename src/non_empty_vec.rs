use thiserror::Error;

/// A vector with at least one element.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NonEmptyVec<T>(Vec<T>);

impl<T> NonEmptyVec<T> {
    pub(crate) fn first(&self) -> &T {
        self.0
            .first()
            .expect("NonEmptyVec always contains at least one element")
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        std::iter::once(self.first()).chain(self.0[1..].iter())
    }
}

impl<T> TryFrom<Vec<T>> for NonEmptyVec<T> {
    type Error = EmptyVecError;

    fn try_from(values: Vec<T>) -> Result<Self, Self::Error> {
        if values.is_empty() {
            Err(EmptyVecError)
        } else {
            Ok(Self(values))
        }
    }
}

impl<T> From<NonEmptyVec<T>> for Vec<T> {
    fn from(values: NonEmptyVec<T>) -> Self {
        values.0
    }
}

impl<T> IntoIterator for NonEmptyVec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a NonEmptyVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("cannot construct NonEmptyVec from an empty Vec")]
pub(crate) struct EmptyVecError;

#[cfg(test)]
mod tests {
    use super::{EmptyVecError, NonEmptyVec};

    #[test]
    fn rejects_empty_vectors() {
        let error = NonEmptyVec::<i32>::try_from(Vec::new()).unwrap_err();

        assert_eq!(error, EmptyVecError);
    }

    #[test]
    fn accepts_a_single_element() {
        let values = NonEmptyVec::try_from(vec![1]).unwrap();

        assert_eq!(values.first(), &1);
        assert_eq!(values.iter().copied().collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn preserves_the_first_element_and_order() {
        let values = NonEmptyVec::try_from(vec![1, 1, 2]).unwrap();

        assert_eq!(values.first(), &1);
        assert_eq!(values.iter().copied().collect::<Vec<_>>(), [1, 1, 2]);
    }

    #[test]
    fn converts_back_to_a_vector() {
        let values = NonEmptyVec::try_from(vec![1, 2, 3]).unwrap();

        assert_eq!(Vec::from(values), [1, 2, 3]);
    }
}
