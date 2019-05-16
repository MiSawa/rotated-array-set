#![doc(html_root_url = "https://senderista.github.io/sorted-vec/")]
#![doc(html_logo_url = "https://raw.githubusercontent.com/senderista/sorted-vec/master/cells.png")]
#![feature(copy_within)]
#![feature(is_sorted)]
#![feature(iter_nth_back)]
#![feature(const_int_conversion)]

use std::cmp::min;
use std::fmt::Debug;
use std::iter::{DoubleEndedIterator, ExactSizeIterator, FromIterator, FusedIterator};

/// A set based on a 2-level rotated array.
///
/// See <a href="https://github.com/senderista/sorted-vec/blob/master/README.md">the repository README</a> for a detailed discussion of this collection's performance
/// benefits and drawbacks.
///
/// # Examples
///
/// ```
/// use sorted_vec::SortedVec;
///
/// // Type inference lets us omit an explicit type signature (which
/// // would be `SortedVec<i32>` in this example).
/// let mut ints = SortedVec::new();
///
/// // Add some integers.
/// ints.insert(-1);
/// ints.insert(6);
/// ints.insert(1729);
/// ints.insert(24);
///
/// // Check for a specific one.
/// if !ints.contains(&42) {
///     println!("We don't have the answer to Life, the Universe, and Everything :-(");
/// }
///
/// // Remove an integer.
/// ints.remove(&6);
///
/// // Iterate over everything.
/// for int in &ints {
///     println!("{}", int);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct SortedVec<T> {
    data: Vec<T>,
    min_indexes: Vec<usize>,
    min_data: Vec<T>,
}

/// An iterator over the items of a `SortedVec`.
///
/// This `struct` is created by the [`iter`] method on [`SortedVec`][`SortedVec`].
/// See its documentation for more.
#[derive(Debug, Copy, Clone)]
pub struct Iter<'a, T: 'a> {
    container: &'a SortedVec<T>,
    next_index: usize,
    next_rev_index: usize,
}

/// An owning iterator over the items of a `SortedVec`.
///
/// This `struct` is created by the [`into_iter`] method on [`SortedVec`][`SortedVec`]
/// (provided by the `IntoIterator` trait). See its documentation for more.
///
/// [`SortedVec`]: struct.SortedVec.html
/// [`into_iter`]: struct.SortedVec.html#method.into_iter
#[derive(Debug, Clone)]
pub struct IntoIter<T> {
    vec: Vec<T>,
    next_index: usize,
}

impl<T> SortedVec<T>
where
    T: Ord + Copy + Default + Debug,
{
    /// Makes a new `SortedVec` without any heap allocations.
    ///
    /// This is a constant-time operation.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![allow(unused_mut)]
    /// use sorted_vec::SortedVec;
    ///
    /// let mut set: SortedVec<i32> = SortedVec::new();
    /// ```
    pub fn new() -> Self {
        SortedVec {
            data: Vec::new(),
            min_indexes: Vec::new(),
            min_data: Vec::new(),
        }
    }

    /// Clears the set, removing all values.
    ///
    /// This is a constant-time operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use sorted_vec::SortedVec;
    ///
    /// let mut v = SortedVec::new();
    /// v.insert(1);
    /// v.clear();
    /// assert!(v.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.data.clear();
        self.min_indexes.clear();
        self.min_data.clear();
    }

    /// Returns `true` if the set contains a value.
    ///
    /// This is an O(lg n) operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use sorted_vec::SortedVec;
    ///
    /// let set: SortedVec<_> = vec![1, 2, 3].into();
    /// assert_eq!(set.contains(&1), true);
    /// assert_eq!(set.contains(&4), false);
    /// ```
    pub fn contains(&self, value: &T) -> bool {
        self.get(value).is_some()
    }

    /// Returns a reference to the value in the set, if any, that is equal to the given value.
    ///
    /// This is an O(lg n) operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use sorted_vec::SortedVec;
    ///
    /// let set: SortedVec<_> = vec![1, 2, 3].into();
    /// assert_eq!(set.get(&2), Some(&2));
    /// assert_eq!(set.get(&4), None);
    /// ```
    pub fn get(&self, value: &T) -> Option<&T> {
        let real_idx = self.find_real_index(value).ok()?;
        Some(&self.data[real_idx])
    }

    ///
    /// Returns the rank of the value in the set if it exists (as Result::Ok),
    /// or the rank of its largest predecessor plus one, if it does not exist (as Result::Err).
    /// This is a constant-time operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use sorted_vec::SortedVec;
    ///
    /// let set: SortedVec<_> = vec![1, 2, 3].into();
    /// assert_eq!(set.rank(&1), Ok(0));
    /// assert_eq!(set.rank(&4), Err(3));
    /// ```
    pub fn rank(&self, value: &T) -> Result<usize, usize> {
        let (real_index, exists) = match self.find_real_index(value) {
            Ok(index) => (index, true),
            Err(index) => (index, false),
        };
        if real_index == self.data.len() {
            return Err(real_index);
        }
        debug_assert!(real_index < self.data.len());
        let subarray_idx = Self::get_subarray_idx_from_array_idx(real_index);
        let subarray_start_idx = Self::get_array_idx_from_subarray_idx(subarray_idx);
        let subarray_len = if subarray_idx == self.min_indexes.len() - 1 {
            self.data.len() - subarray_start_idx
        } else {
            subarray_idx + 1
        };
        let pivot_idx = subarray_start_idx + self.min_indexes[subarray_idx];
        let logical_index = if real_index >= pivot_idx {
            subarray_start_idx + real_index - pivot_idx
        } else {
            subarray_start_idx + subarray_len - (pivot_idx - real_index)
        };
        if exists {
            Ok(logical_index)
        } else {
            Err(logical_index)
        }
    }

    /// Returns a reference to the value in the set, if any, with the given rank.
    ///
    /// This is a constant-time operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use sorted_vec::SortedVec;
    ///
    /// let set: SortedVec<_> = vec![1, 2, 3].into();
    /// assert_eq!(set.select(0), Some(&1));
    /// assert_eq!(set.select(3), None);
    /// ```
    pub fn select(&self, rank: usize) -> Option<&T> {
        if rank >= self.data.len() {
            return None;
        }
        let subarray_idx = Self::get_subarray_idx_from_array_idx(rank);
        let subarray_start_idx = Self::get_array_idx_from_subarray_idx(subarray_idx);
        let subarray_len = if subarray_idx == self.min_indexes.len() - 1 {
            self.data.len() - subarray_start_idx
        } else {
            subarray_idx + 1
        };
        debug_assert!(rank >= subarray_start_idx);
        let idx_offset = rank - subarray_start_idx;
        let pivot_offset = self.min_indexes[subarray_idx];
        let rotated_offset = (pivot_offset + idx_offset) % subarray_len;
        debug_assert!(rotated_offset < subarray_len);
        let real_idx = subarray_start_idx + rotated_offset;
        Some(&self.data[real_idx])
    }

    /// Adds a value to the set.
    ///
    /// This is an O(√n) operation.
    ///
    /// If the set did not have this value present, `true` is returned.
    ///
    /// If the set did have this value present, `false` is returned, and the
    /// entry is not updated.
    ///
    /// # Examples
    ///
    /// ```
    /// use sorted_vec::SortedVec;
    ///
    /// let mut set = SortedVec::new();
    ///
    /// assert_eq!(set.insert(2), true);
    /// assert_eq!(set.insert(2), false);
    /// assert_eq!(set.len(), 1);
    /// ```
    pub fn insert(&mut self, value: T) -> bool {
        let insert_idx = match self.find_real_index(&value).err() {
            None => return false,
            Some(idx) => idx,
        };
        // find subarray containing this insertion point
        let subarray_idx = Self::get_subarray_idx_from_array_idx(insert_idx);
        // inserted element could be in a new subarray
        debug_assert!(subarray_idx <= self.min_indexes.len());
        // create a new subarray if necessary
        if subarray_idx == self.min_indexes.len() {
            self.min_indexes.push(0);
            self.min_data.push(T::default());
        }
        debug_assert_eq!(self.min_indexes.len(), self.min_data.len());
        let subarray_offset = Self::get_array_idx_from_subarray_idx(subarray_idx);
        // if insertion point is in last subarray and last subarray isn't full, just insert the new element
        if subarray_idx == self.min_indexes.len() - 1 && !self.is_last_subarray_full() {
            // Since we always insert into a partially full subarray in sorted order,
            // there is no need to update the pivot location, but we do have to update
            // the pivot value.
            debug_assert!(self.min_indexes[subarray_idx] == 0);
            self.data.insert(insert_idx, value);
            // These writes are redundant unless the minimum has changed, but avoiding a branch may be worth it,
            // given that the end of the data arrays should be in cache.
            self.min_data[subarray_idx] = self.data[subarray_offset];
            debug_assert!(self.assert_invariants());
            return true;
        }
        // From now on, we can assume that the subarray we're inserting into is always full.
        let next_subarray_offset = Self::get_array_idx_from_subarray_idx(subarray_idx + 1);
        let subarray = &mut self.data[subarray_offset..next_subarray_offset];
        let pivot_offset = self.min_indexes[subarray_idx];
        let insert_offset = insert_idx - subarray_offset;
        let max_offset = if pivot_offset == 0 {
            subarray.len() - 1
        } else {
            pivot_offset - 1
        };
        let mut prev_max = subarray[max_offset];
        // this logic is best understood with a diagram of a rotated array, e.g.:
        //
        // ------------------------------------------------------------------------
        // | 12 | 13 | 14 | 15 | 16 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 |
        // ------------------------------------------------------------------------
        //
        if max_offset < pivot_offset && insert_offset >= pivot_offset {
            subarray.copy_within(pivot_offset..insert_offset, max_offset);
            subarray[insert_offset - 1] = value;
            self.min_indexes[subarray_idx] = max_offset;
            self.min_data[subarray_idx] = subarray[max_offset];
        } else {
            subarray.copy_within(insert_offset..max_offset, insert_offset + 1);
            subarray[insert_offset] = value;
            if insert_offset == pivot_offset {
                // inserted value is new minimum for subarray
                self.min_data[subarray_idx] = value;
            }
        }
        debug_assert!(self.assert_invariants());
        let max_subarray_idx = self.min_indexes.len() - 1;
        let next_subarray_idx = subarray_idx + 1;
        let last_subarray_full = self.is_last_subarray_full();
        // now loop over all remaining subarrays, setting the min (pivot) of each to the max of its predecessor
        for (i, pivot_offset_ref) in self.min_indexes[next_subarray_idx..].iter_mut().enumerate() {
            let cur_subarray_idx = next_subarray_idx + i;
            // if the last subarray isn't full, skip it
            if cur_subarray_idx == max_subarray_idx && !last_subarray_full {
                break;
            }
            let max_offset = if *pivot_offset_ref == 0 {
                cur_subarray_idx
            } else {
                *pivot_offset_ref - 1
            };
            let max_idx = max_offset + Self::get_array_idx_from_subarray_idx(cur_subarray_idx);
            let next_max = self.data[max_idx];
            self.data[max_idx] = prev_max;
            *pivot_offset_ref = max_offset;
            self.min_data[cur_subarray_idx] = prev_max;
            prev_max = next_max;
        }
        // if the last subarray was full, append current max to a new subarray, otherwise insert max in sorted order
        if last_subarray_full {
            self.data.push(prev_max);
            self.min_indexes.push(0);
            self.min_data.push(prev_max);
        } else {
            let max_subarray_offset = Self::get_array_idx_from_subarray_idx(max_subarray_idx);
            // since `max` is guaranteed to be <= the pivot value, we always insert it at the pivot location
            debug_assert!(prev_max <= self.data[max_subarray_offset]);
            self.data.insert(max_subarray_offset, prev_max);
            self.min_data[max_subarray_idx] = prev_max;
        }
        debug_assert!(self.find_real_index(&value).is_ok());
        debug_assert!(self.assert_invariants());
        true
    }

    /// Removes a value from the set. Returns whether the value was
    /// present in the set.
    ///
    /// This is an O(√n) operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use sorted_vec::SortedVec;
    ///
    /// let mut set = SortedVec::new();
    ///
    /// set.insert(2);
    /// assert_eq!(set.remove(&2), true);
    /// assert_eq!(set.remove(&2), false);
    /// ```
    pub fn remove(&mut self, value: &T) -> bool {
        let mut remove_idx = match self.find_real_index(&value).ok() {
            Some(idx) => idx,
            None => return false,
        };
        let max_subarray_idx = self.min_indexes.len() - 1;
        let max_subarray_offset = Self::get_array_idx_from_subarray_idx(max_subarray_idx);
        // find subarray containing the element to remove
        let subarray_idx = Self::get_subarray_idx_from_array_idx(remove_idx);
        debug_assert!(subarray_idx <= max_subarray_idx);
        let subarray_offset = Self::get_array_idx_from_subarray_idx(subarray_idx);
        // if we're not removing an element in the last subarray, then we end up deleting its minimum,
        // which is always at the first offset since it's sorted
        let mut max_subarray_remove_idx = if subarray_idx == max_subarray_idx {
            remove_idx
        } else {
            max_subarray_offset
        };
        // if the last subarray was rotated, sort it to maintain insert invariant
        if self.is_last_subarray_full() {
            let last_min_offset = self.min_indexes[max_subarray_idx];
            // rotate left by the min offset instead of sorting
            self.data[max_subarray_offset..].rotate_left(last_min_offset);
            self.min_indexes[max_subarray_idx] = 0;
            // the remove index might change after sorting the last subarray
            if subarray_idx == max_subarray_idx {
                remove_idx = self
                    .find_real_index(&value)
                    .expect("recalculating remove index after sorting");
                max_subarray_remove_idx = remove_idx;
            }
        }
        // if insertion point is not in last subarray, perform a "hard exchange"
        if subarray_idx < max_subarray_idx {
            // From now on, we can assume that the subarray we're removing from is full.
            let next_subarray_offset = Self::get_array_idx_from_subarray_idx(subarray_idx + 1);
            let subarray = &mut self.data[subarray_offset..next_subarray_offset];
            let pivot_offset = self.min_indexes[subarray_idx];
            let remove_offset = remove_idx - subarray_offset;
            let max_offset = if pivot_offset == 0 {
                subarray.len() - 1
            } else {
                pivot_offset - 1
            };
            // this logic is best understood with a diagram of a rotated array, e.g.:
            //
            // ------------------------------------------------------------------------
            // | 12 | 13 | 14 | 15 | 16 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 |
            // ------------------------------------------------------------------------
            //
            let mut prev_max_offset = if max_offset < pivot_offset && remove_offset >= pivot_offset
            {
                subarray.copy_within(pivot_offset..remove_offset, pivot_offset + 1);
                let new_pivot_offset = if pivot_offset == subarray.len() - 1 {
                    0
                } else {
                    pivot_offset + 1
                };
                self.min_indexes[subarray_idx] = new_pivot_offset;
                self.min_data[subarray_idx] = subarray[new_pivot_offset];
                pivot_offset
            } else {
                subarray.copy_within(remove_offset + 1..=max_offset, remove_offset);
                if remove_offset == pivot_offset {
                    self.min_data[subarray_idx] = subarray[pivot_offset];
                }
                max_offset
            };
            let next_subarray_idx = min(max_subarray_idx, subarray_idx + 1);
            // now perform an "easy exchange" in all remaining subarrays except the last,
            // setting the max of each to the min of its successor.
            for (i, pivot_offset_ref) in self.min_indexes[next_subarray_idx..max_subarray_idx]
                .iter_mut()
                .enumerate()
            {
                let cur_subarray_idx = next_subarray_idx + i;
                let cur_subarray_offset = Self::get_array_idx_from_subarray_idx(cur_subarray_idx);
                let prev_max_idx =
                    prev_max_offset + Self::get_array_idx_from_subarray_idx(cur_subarray_idx - 1);
                self.data[prev_max_idx] = self.data[cur_subarray_offset + *pivot_offset_ref];
                // the min_data array needs to be updated when the previous subarray's max offset
                // coincides with its min offset, i.e., when it is subarray 0
                if cur_subarray_idx == 1 {
                    self.min_data[0] = self.data[0];
                    debug_assert!(self.min_data.iter().is_sorted());
                }
                prev_max_offset = *pivot_offset_ref;
                let new_min_offset = if *pivot_offset_ref == cur_subarray_idx {
                    0
                } else {
                    *pivot_offset_ref + 1
                };
                *pivot_offset_ref = new_min_offset;
                self.min_data[cur_subarray_idx] = self.data[cur_subarray_offset + new_min_offset];
                debug_assert!(self.min_data.iter().is_sorted());
            }
            // now we fix up the last subarray. if it was initially full, we need to sort it to maintain the insert invariant.
            // if the removed element is in the last subarray, we just sort and remove() on the vec, updating auxiliary arrays.
            // otherwise, we copy the minimum to the max position of the previous subarray, then remove it and fix up
            // auxiliary arrays.
            let prev_max_idx =
                prev_max_offset + Self::get_array_idx_from_subarray_idx(max_subarray_idx - 1);
            // since the last subarray is always sorted, its minimum element is always on the first offset
            self.data[prev_max_idx] = self.data[max_subarray_offset];
            // the min_data array needs to be updated when the previous subarray's max offset
            // coincides with its min offset, i.e., when it is subarray 0
            if max_subarray_idx == 1 {
                self.min_data[0] = self.data[0];
                debug_assert!(self.min_data.iter().is_sorted());
            }
        }
        self.data.remove(max_subarray_remove_idx);
        // if last subarray is now empty, trim the auxiliary arrays
        if max_subarray_offset == self.data.len() {
            self.min_indexes.pop();
            self.min_data.pop();
        } else {
            // since the last subarray is always sorted, its minimum is always on the first offset
            self.min_data[max_subarray_idx] = self.data[max_subarray_offset];
            debug_assert!(self.min_data.iter().is_sorted());
        }
        debug_assert!(self.find_real_index(&value).is_err());
        debug_assert!(self.assert_invariants());
        true
    }

    /// Removes and returns the value in the set, if any, that is equal to the given one.
    ///
    /// This is an O(√n) operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use sorted_vec::SortedVec;
    ///
    /// let mut set: SortedVec<_> = vec![1, 2, 3].into();
    /// assert_eq!(set.take(&2), Some(2));
    /// assert_eq!(set.take(&2), None);
    /// ```
    pub fn take(&mut self, value: &T) -> Option<T> {
        let ret = self.get(value).copied();
        if ret.is_some() {
            self.remove(value);
        }
        ret
    }

    /// Returns the number of elements in the set.
    ///
    /// This is a constant-time operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use sorted_vec::SortedVec;
    ///
    /// let mut v = SortedVec::new();
    /// assert_eq!(v.len(), 0);
    /// v.insert(1);
    /// assert_eq!(v.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns `true` if the set contains no elements.
    ///
    /// This is a constant-time operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use sorted_vec::SortedVec;
    ///
    /// let mut v = SortedVec::new();
    /// assert!(v.is_empty());
    /// v.insert(1);
    /// assert!(!v.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Gets an iterator that visits the values in the `SortedVec` in ascending order.
    ///
    /// # Examples
    ///
    /// ```
    /// use sorted_vec::SortedVec;
    ///
    /// let set: SortedVec<usize> = vec![1, 2, 3].into();
    /// let mut set_iter = set.iter();
    /// assert_eq!(set_iter.next(), Some(&1));
    /// assert_eq!(set_iter.next(), Some(&2));
    /// assert_eq!(set_iter.next(), Some(&3));
    /// assert_eq!(set_iter.next(), None);
    /// ```
    ///
    /// Values returned by the iterator are returned in ascending order:
    ///
    /// ```
    /// use sorted_vec::SortedVec;
    ///
    /// let set: SortedVec<usize> = vec![3, 1, 2].into();
    /// let mut set_iter = set.iter();
    /// assert_eq!(set_iter.next(), Some(&1));
    /// assert_eq!(set_iter.next(), Some(&2));
    /// assert_eq!(set_iter.next(), Some(&3));
    /// assert_eq!(set_iter.next(), None);
    /// ```
    pub fn iter(&self) -> Iter<T> {
        Iter {
            container: self,
            next_index: 0,
            next_rev_index: self.len() - 1,
        }
    }

    fn integer_sum(n: usize) -> usize {
        // I learned this from a 10-year-old named Gauss
        (n * (n + 1)) / 2
    }

    fn integer_sum_inverse(n: usize) -> usize {
        // y = (x * (x + 1)) / 2
        // x = (sqrt(8 * y + 1) - 1) / 2
        ((f64::from((n * 8 + 1) as u32).sqrt() as usize) - 1) / 2
    }

    fn get_subarray_idx_from_array_idx(idx: usize) -> usize {
        if idx == 0 {
            0
        } else {
            Self::integer_sum_inverse(idx)
        }
    }

    fn get_array_idx_from_subarray_idx(idx: usize) -> usize {
        if idx == 0 {
            0
        } else {
            Self::integer_sum(idx)
        }
    }

    fn is_last_subarray_full(&self) -> bool {
        self.data.len() == Self::get_array_idx_from_subarray_idx(self.min_indexes.len())
    }

    // Returns either (raw) index of element if it exists, or (raw) insertion point if it doesn't exist.
    fn find_real_index(&self, value: &T) -> Result<usize, usize> {
        if self.data.is_empty() {
            return Err(0);
        }
        // find two candidate subarrays by binary searching self.min_data,
        // then compare value to max value of first subarray, if it's smaller
        // then binary search first subarray, otherwise second subarray
        // TODO: actually we only need to binary search first subarray, max
        // comparison is just to determine insertion point (to preserve invariant
        // that we never insert element into a subarray greater than its current max).
        // if element greater than max of first subarray but less than min of
        // second subarray, just return insertion point on min index of second subarray.
        debug_assert!(self.assert_invariants());
        match self.min_data.binary_search(value) {
            Ok(idx) => {
                // `value` is located directly on a pivot index
                let found_idx = Self::get_array_idx_from_subarray_idx(idx) + self.min_indexes[idx];
                Ok(found_idx)
            }
            Err(idx) => {
                // The element might be in either the subarray corresponding to the insertion point,
                // or in its predecessor; compare to max value of predecessor to decide.
                // A special case is when the insertion point is after the last subarray and the last subarray isn't full.
                // In that case, we want to insert into the existing last subarray, not create a new one.
                let subarray_idx = if idx == 0 {
                    0
                } else if idx == self.min_indexes.len() && !self.is_last_subarray_full() {
                    // partially full final subarray
                    idx - 1
                } else {
                    // we can assume the predecessor subarray is full
                    let prev_max_idx = if self.min_indexes[idx - 1] == 0 {
                        Self::get_array_idx_from_subarray_idx(idx) - 1
                    } else {
                        Self::get_array_idx_from_subarray_idx(idx - 1) + self.min_indexes[idx - 1]
                            - 1
                    };
                    if *value <= self.data[prev_max_idx] {
                        idx - 1
                    } else {
                        idx
                    }
                };
                let subarray_offset = Self::get_array_idx_from_subarray_idx(subarray_idx);
                // we may need to create a new subarray to insert this element
                debug_assert!(subarray_offset <= self.data.len());
                if subarray_offset == self.data.len() {
                    return Err(subarray_offset);
                }
                // if our last subarray is truncated, then account for that
                let next_subarray_offset = if subarray_idx == self.min_indexes.len() - 1 {
                    self.data.len()
                } else {
                    Self::get_array_idx_from_subarray_idx(subarray_idx + 1)
                };
                // split subarray into two slices separated by pivot,
                // and search both separately.
                let subarray = &self.data[subarray_offset..next_subarray_offset];
                let pivot_offset = self.min_indexes[subarray_idx];
                let subarray_pivot = subarray_offset + pivot_offset;
                let (left, right) = subarray.split_at(pivot_offset);
                debug_assert!(left.iter().is_sorted() && right.iter().is_sorted());
                match (left.binary_search(value), right.binary_search(value)) {
                    (Ok(idx), _) => Ok(subarray_offset + idx),
                    (_, Ok(idx)) => Ok(subarray_pivot + idx),
                    // if right insertion point is past right subarray, and left subarray is not empty, then true insertion point must be on left
                    (Err(left_idx), Err(right_idx))
                        if right_idx == right.len() && !left.is_empty() =>
                    {
                        Err(subarray_offset + left_idx)
                    }
                    // if right insertion point is within right subarray, or left subarray is empty, then true insertion point must be on right
                    (Err(_left_idx), Err(right_idx))
                        if right_idx < right.len() || left.is_empty() =>
                    {
                        Err(subarray_pivot + right_idx)
                    }
                    (Err(_), Err(_)) => unreachable!(),
                }
            }
        }
    }

    #[inline(always)]
    fn assert_invariants(&self) -> bool {
        // assert order
        assert!(self.min_data.iter().is_sorted());
        let mut min_data_dedup = self.min_data.clone();
        min_data_dedup.dedup();
        // assert uniqueness
        assert!(self.min_data[..] == min_data_dedup[..]);
        // assert index of each subarray's minimum lies within the subarray
        assert!(self
            .min_indexes
            .iter()
            .enumerate()
            .all(|(idx, &offset)| offset <= idx));
        // assert min_data is properly synchronized with min_indexes and self.data
        assert!(self
            .min_indexes
            .iter()
            .enumerate()
            .all(|(idx, &offset)| self.min_data[idx]
                == self.data[Self::get_array_idx_from_subarray_idx(idx) + offset]));
        // assert min_indexes holds the index of the actual minimum of each subarray
        for i in 0..self.min_indexes.len() {
            let subarray_begin_idx = Self::get_array_idx_from_subarray_idx(i);
            let subarray_end_idx = min(
                self.data.len(),
                Self::get_array_idx_from_subarray_idx(i + 1),
            );
            let subarray = &self.data[subarray_begin_idx..subarray_end_idx];
            let min_idx = subarray
                .iter()
                .enumerate()
                .min_by(|&(_, v1), &(_, v2)| v1.cmp(v2))
                .unwrap()
                .0;
            assert!(min_idx == self.min_indexes[i]);
        }
        true
    }

    // given data array, initialize auxiliary arrays
    fn init(&mut self) {
        debug_assert!(!self.data.is_empty());
        self.data.sort_unstable(); // don't want to allocate
        let last_subarray_idx = Self::get_subarray_idx_from_array_idx(self.data.len() - 1);
        self.min_indexes = vec![0; last_subarray_idx + 1];
        for subarray_idx in 0..=last_subarray_idx {
            let subarray_offset = Self::get_array_idx_from_subarray_idx(subarray_idx);
            self.min_data.push(self.data[subarray_offset]);
        }
    }
}

impl<'a, T> Iterator for Iter<'a, T>
where
    T: Ord + Copy + Default + Debug,
{
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        if self.next_index > self.next_rev_index {
            None
        } else {
            let current = self.container.select(self.next_index);
            self.next_index += 1;
            current
        }
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.next_index += n;
        if self.next_index > self.next_rev_index {
            None
        } else {
            let nth = self.container.select(self.next_index);
            self.next_index += 1;
            nth
        }
    }

    fn count(self) -> usize {
        self.container.data.len() - self.next_index
    }

    fn last(self) -> Option<Self::Item> {
        self.container.select(self.container.data.len() - 1)
    }

    fn max(self) -> Option<Self::Item> {
        self.container.select(self.len() - 1)
    }

    fn min(self) -> Option<Self::Item> {
        self.container.select(0)
    }

    fn is_sorted(self) -> bool {
        true
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining_count = self.container.data.len() - self.next_index;
        (remaining_count, Some(remaining_count))
    }
}

impl<'a, T> DoubleEndedIterator for Iter<'a, T>
where
    T: Ord + Copy + Default + Debug,
{
    fn next_back(&mut self) -> Option<&'a T> {
        if self.next_rev_index < self.next_index {
            None
        } else {
            let current = self.container.select(self.next_rev_index);
            self.next_rev_index -= 1;
            current
        }
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.next_rev_index -= n;
        if self.next_rev_index < self.next_index {
            None
        } else {
            let nth = self.container.select(self.next_rev_index);
            self.next_rev_index -= 1;
            nth
        }
    }
}

impl<T> ExactSizeIterator for Iter<'_, T>
where
    T: Ord + Copy + Default + Debug,
{
    fn len(&self) -> usize {
        self.container.len()
    }
}

impl<T> FusedIterator for Iter<'_, T> where T: Ord + Copy + Default + Debug {}

impl<'a, T> IntoIterator for &'a SortedVec<T>
where
    T: Ord + Copy + Default + Debug,
{
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

impl<T> IntoIterator for SortedVec<T>
where
    T: Ord + Copy + Default + Debug,
{
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            vec: self.into(),
            next_index: 0,
        }
    }
}

impl<'a, T> Iterator for IntoIter<T>
where
    T: Ord + Copy + Default + Debug,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.vec.len() {
            None
        } else {
            let current = self.vec[self.next_index];
            self.next_index += 1;
            Some(current)
        }
    }
}

impl<'a, T> From<&'a [T]> for SortedVec<T>
where
    T: Ord + Copy + Default + Debug,
{
    fn from(slice: &[T]) -> Self {
        let mut this = SortedVec {
            data: slice.to_owned(),
            min_indexes: Vec::new(),
            min_data: Vec::new(),
        };
        this.init();
        this
    }
}

impl<T> From<Vec<T>> for SortedVec<T>
where
    T: Ord + Copy + Default + Debug,
{
    fn from(vec: Vec<T>) -> Self {
        let mut this = SortedVec {
            data: vec,
            min_indexes: Vec::new(),
            min_data: Vec::new(),
        };
        this.init();
        this
    }
}

impl<T> Into<Vec<T>> for SortedVec<T>
where
    T: Ord + Copy + Default + Debug,
{
    fn into(mut self) -> Vec<T> {
        // sort the data array in-place and steal it from self
        for (i, &pivot_offset) in self.min_indexes.iter().enumerate() {
            let subarray_start_idx = Self::get_array_idx_from_subarray_idx(i);
            let subarray_len = if i == self.min_indexes.len() - 1 {
                self.data.len() - subarray_start_idx
            } else {
                i + 1
            };
            let subarray_end_idx = subarray_start_idx + subarray_len;
            let subarray = &mut self.data[subarray_start_idx..subarray_end_idx];
            // sort subarray in-place
            subarray.rotate_left(pivot_offset);
        }
        self.data
    }
}

impl<T> FromIterator<T> for SortedVec<T>
where
    T: Ord + Copy + Default + Debug,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut this = SortedVec {
            data: Vec::from_iter(iter.into_iter()),
            min_indexes: Vec::new(),
            min_data: Vec::new(),
        };
        this.init();
        this
    }
}

impl<T: Ord> Default for SortedVec<T>
where
    T: Ord + Copy + Default + Debug,
{
    fn default() -> SortedVec<T> {
        SortedVec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::distributions::Standard;
    use rand::prelude::*;
    use rand::rngs::SmallRng;

    const NUM_ELEMS: usize = 1 << 10;
    const SEED: u64 = u64::from_be_bytes(*b"cafebabe");

    #[test]
    fn insert_remove() {
        let mut rng: SmallRng = SeedableRng::seed_from_u64(SEED);
        let iter = rng.sample_iter(&Standard).take(NUM_ELEMS);
        let mut sorted_vec: SortedVec<usize> = SortedVec::new();
        for v in iter {
            assert!(sorted_vec.insert(v));
        }
        let mut rng: SmallRng = SeedableRng::seed_from_u64(SEED);
        let iter = rng.sample_iter(&Standard).take(NUM_ELEMS);
        for v in iter {
            assert!(sorted_vec.remove(&v));
        }
        assert!(sorted_vec.is_empty());
    }

    #[test]
    fn rank_select() {
        let mut rng: SmallRng = SeedableRng::seed_from_u64(SEED);
        let iter = rng.sample_iter(&Standard).take(NUM_ELEMS);
        let mut sorted_vec: SortedVec<usize> = SortedVec::new();
        for v in iter {
            assert!(sorted_vec.insert(v));
        }
        let mut rng: SmallRng = SeedableRng::seed_from_u64(SEED);
        let iter = rng.sample_iter(&Standard).take(NUM_ELEMS);
        for v1 in iter {
            let p = sorted_vec.rank(&v1).unwrap();
            let v2 = *sorted_vec.select(p).unwrap();
            assert!(v1 == v2);
        }
    }

    #[test]
    fn compare_iter() {
        let mut rng: SmallRng = SeedableRng::seed_from_u64(SEED);
        let iter = rng.sample_iter(&Standard).take(NUM_ELEMS);
        let mut sorted_vec: SortedVec<usize> = SortedVec::new();
        for v in iter {
            assert!(sorted_vec.insert(v));
        }
        let iter = sorted_vec.iter();
        for (i, &v) in iter.enumerate() {
            assert!(*sorted_vec.select(i).unwrap() == v);
        }
    }

    #[test]
    fn compare_into_iter() {
        let mut rng: SmallRng = SeedableRng::seed_from_u64(SEED);
        let iter = rng.sample_iter(&Standard).take(NUM_ELEMS as usize);
        let mut sorted_vec: SortedVec<usize> = SortedVec::new();
        for v in iter {
            assert!(sorted_vec.insert(v));
        }
        let mut iter = sorted_vec.clone().into_iter();
        for i in 0..NUM_ELEMS {
            assert!(*sorted_vec.select(i).unwrap() == iter.next().unwrap());
        }
    }

    #[test]
    fn test_iter_overrides() {
        let sorted_vec: SortedVec<_> = (0usize..NUM_ELEMS).collect();
        let iter = sorted_vec.iter();
        assert!(*iter.min().unwrap() == *sorted_vec.select(0).unwrap());
        assert!(*iter.max().unwrap() == *sorted_vec.select(NUM_ELEMS - 1).unwrap());
        assert!(*iter.last().unwrap() == *sorted_vec.select(NUM_ELEMS - 1).unwrap());
        assert!(iter.count() == sorted_vec.len());
        assert!(*iter.last().unwrap() == *sorted_vec.select(NUM_ELEMS - 1).unwrap());
        let step = NUM_ELEMS / 10;
        let mut iter_nth = iter;
        assert!(*iter_nth.nth(step - 1).unwrap() == *sorted_vec.select(step - 1).unwrap());
        assert!(*iter_nth.nth(step - 1).unwrap() == *sorted_vec.select((2 * step) - 1).unwrap());
        let mut iter_nth_back = iter;
        let last_index = sorted_vec.len() - 1;
        assert!(*iter_nth_back.nth_back(step - 1).unwrap() == *sorted_vec.select(last_index - step + 1).unwrap());
        assert!(*iter_nth_back.nth_back(step - 1).unwrap() == *sorted_vec.select(last_index - (2 * step) + 1).unwrap());
        let mut iter_mut = sorted_vec.iter();
        for i in 0..(NUM_ELEMS / 2) {
            assert!(*iter_mut.next().unwrap() == *sorted_vec.select(i).unwrap());
            assert!(*iter_mut.next_back().unwrap() == *sorted_vec.select(last_index - i).unwrap());
        }
        assert!(iter_mut.next().is_none());
    }
}
