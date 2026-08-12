use std::{
    iter::{self, Chain, Once},
    slice::Iter,
    vec::IntoIter,
};
use thiserror::Error;

/// A vector with at least one element.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NonEmptyVec<T> {
    first: T,
    rest: Vec<T>,
}

impl<T> NonEmptyVec<T> {
    pub(crate) fn first(&self) -> &T {
        &self.first
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        iter::once(&self.first).chain(self.rest.iter())
    }
}

impl<T> TryFrom<Vec<T>> for NonEmptyVec<T> {
    type Error = EmptyVecError;

    fn try_from(values: Vec<T>) -> Result<Self, Self::Error> {
        let mut values = values.into_iter();
        let first = values.next().ok_or(EmptyVecError)?;
        let rest = values.collect();
        Ok(Self { first, rest })
    }
}

impl<T> From<NonEmptyVec<T>> for Vec<T> {
    fn from(values: NonEmptyVec<T>) -> Self {
        iter::once(values.first).chain(values.rest).collect()
    }
}

impl<T> IntoIterator for NonEmptyVec<T> {
    type Item = T;
    type IntoIter = Chain<Once<T>, IntoIter<T>>;

    fn into_iter(self) -> Self::IntoIter {
        iter::once(self.first).chain(self.rest)
    }
}

impl<'a, T> IntoIterator for &'a NonEmptyVec<T> {
    type Item = &'a T;
    type IntoIter = Chain<Once<&'a T>, Iter<'a, T>>;

    fn into_iter(self) -> Self::IntoIter {
        iter::once(&self.first).chain(self.rest.iter())
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
    fn iterates_over_borrowed_elements_in_order() {
        let values = NonEmptyVec::try_from(vec![1, 2, 3]).unwrap();

        assert_eq!(
            (&values).into_iter().copied().collect::<Vec<_>>(),
            [1, 2, 3]
        );
    }

    #[test]
    fn iterates_over_owned_elements_in_order() {
        let values = NonEmptyVec::try_from(vec![1, 2, 3]).unwrap();

        assert_eq!(values.into_iter().collect::<Vec<_>>(), [1, 2, 3]);
    }

    #[test]
    fn converts_back_to_a_vector() {
        let values = NonEmptyVec::try_from(vec![1, 2, 3]).unwrap();

        assert_eq!(Vec::from(values), [1, 2, 3]);
    }
}
