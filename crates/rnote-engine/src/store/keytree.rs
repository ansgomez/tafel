// Imports
use super::StrokeKey;
use p2d::bounding_volume::Aabb;
use p2d::math::Vector2;
use rstar::primitives::GeomWithData;

/// The rtree object that holds the bounds and [StrokeKey].
type KeyTreeObject = GeomWithData<rstar::primitives::Rectangle<[f64; 2]>, StrokeKey>;

#[derive(Debug, Default)]
/// A Rtree with [StrokeKey]'s as associated data.
///
/// Used for faster spatial queries.
pub(super) struct KeyTree(rstar::RTree<KeyTreeObject, rstar::DefaultParams>);

impl KeyTree {
    /// Insert a new tree object with the given [StrokeKey] and bounds.
    ///
    /// Bounds that are not finite are not inserted, because they corrupt the tree, which then
    /// panics on later insertions.
    pub(crate) fn insert_with_key(&mut self, key: StrokeKey, bounds: Aabb) {
        if !bounds_are_valid(&bounds) {
            tracing::warn!(
                "Not inserting stroke with key {key:?} into the key tree, its bounds are not finite: {bounds:?}"
            );
            return;
        }
        self.0.insert(new_keytree_object(key, bounds));
    }

    /// Removes the [KeyTreeObject] for the given key.
    pub(crate) fn remove_with_key(&mut self, key: StrokeKey) -> Option<KeyTreeObject> {
        let object_to_remove = self.0.iter().find(|&object| object.data == key)?.to_owned();

        self.0.remove(&object_to_remove)
    }

    /// Update the Tree with new bounds for the given key.
    ///
    /// Has to be called when the geometry of the stroke has changed.
    pub(crate) fn update_with_key(&mut self, key: StrokeKey, new_bounds: Aabb) {
        self.remove_with_key(key);
        self.insert_with_key(key, new_bounds);
    }

    /// Return the keys that intersect with the given bounds.
    pub(crate) fn keys_intersecting_bounds(&self, bounds: Aabb) -> Vec<StrokeKey> {
        self.0
            .locate_in_envelope_intersecting(rstar::AABB::from_corners(
                [bounds.mins[0], bounds.mins[1]],
                [bounds.maxs[0], bounds.maxs[1]],
            ))
            .map(|object| object.data)
            .collect()
    }

    /// Return the keys that are completely contained in the given bounds.
    pub(crate) fn keys_in_bounds(&self, bounds: Aabb) -> Vec<StrokeKey> {
        self.0
            .locate_in_envelope(rstar::AABB::from_corners(
                [bounds.mins[0], bounds.mins[1]],
                [bounds.maxs[0], bounds.maxs[1]],
            ))
            .map(|object| object.data)
            .collect()
    }

    /// Get containing bounds for the supplied keys.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (StrokeKey, Aabb)> {
        self.0.iter().map(|geom| {
            let bounds = geom.geom();
            let lower = bounds.lower();
            let upper = bounds.upper();
            (
                geom.data,
                Aabb::new(
                    Vector2::new(lower[0], lower[1]),
                    Vector2::new(upper[0], upper[1]),
                ),
            )
        })
    }

    /// Rebuild the entire rtree from the given Vec of (key, bounds).
    pub(crate) fn rebuild_from_vec(&mut self, strokes: Vec<(StrokeKey, Aabb)>) {
        let (valid, invalid): (Vec<_>, Vec<_>) = strokes
            .into_iter()
            .partition(|(_, bounds)| bounds_are_valid(bounds));
        for (key, bounds) in invalid {
            tracing::warn!(
                "Not inserting stroke with key {key:?} into the key tree, its bounds are not finite: {bounds:?}"
            );
        }
        let objects = valid
            .iter()
            .map(|&(key, bounds)| new_keytree_object(key, bounds))
            .collect();
        self.0 = rstar::RTree::bulk_load(objects);

        // rstar 0.13.0 bulk loading can create a tree with objects at different depths (e.g. for
        // 25..=27 or 97..=111 objects). Inserting into such a tree panics with
        // "This is a bug in rstar.", so it is then built by inserting one object at a time.
        if !self.is_balanced() {
            tracing::debug!("Bulk loaded key tree is unbalanced, inserting one by one instead");
            self.clear();
            for (key, bounds) in valid {
                self.0.insert(new_keytree_object(key, bounds));
            }
        }
    }

    /// Whether all objects are stored at the same depth, as rstar requires for insertions.
    pub(crate) fn is_balanced(&self) -> bool {
        fn leaves_at_depth(
            node: &rstar::ParentNode<KeyTreeObject>,
            depth: usize,
            leaf_depth: &mut Option<usize>,
        ) -> bool {
            node.children().iter().all(|child| match child {
                rstar::RTreeNode::Leaf(_) => *leaf_depth.get_or_insert(depth) == depth,
                rstar::RTreeNode::Parent(parent) => leaves_at_depth(parent, depth + 1, leaf_depth),
            })
        }
        leaves_at_depth(self.0.root(), 0, &mut None)
    }

    ///  Clear the entire tree.
    pub(crate) fn clear(&mut self) {
        *self = Self::default()
    }
}

/// Whether the bounds can be stored in the tree: all coordinates must be finite.
fn bounds_are_valid(bounds: &Aabb) -> bool {
    bounds.mins.is_finite() && bounds.maxs.is_finite()
}

fn new_keytree_object(key: StrokeKey, bounds: Aabb) -> KeyTreeObject {
    KeyTreeObject::new(
        rstar::primitives::Rectangle::from_corners(
            [bounds.mins[0], bounds.mins[1]],
            [bounds.maxs[0], bounds.maxs[1]],
        ),
        key,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;

    fn rect(i: u64) -> Aabb {
        let x = (i * 37 % 600) as f64;
        let y = (i * 91 % 1700) as f64;
        Aabb::new(Vector2::new(x, y), Vector2::new(x + 20.0, y + 20.0))
    }

    #[test]
    fn insert_after_rebuild_does_not_panic() {
        // rstar 0.13.0 bulk loading creates unbalanced trees for some sizes (e.g. 25 and 100),
        // inserting into them panicked.
        for n in [1, 6, 7, 25, 26, 27, 50, 97, 100, 111, 150] {
            let mut keys = SlotMap::<StrokeKey, ()>::with_key();
            let mut tree = KeyTree::default();
            let strokes = (0..n)
                .map(|i| (keys.insert(()), rect(i)))
                .collect::<Vec<_>>();
            tree.rebuild_from_vec(strokes.clone());
            assert!(tree.is_balanced(), "unbalanced tree for {n} objects");
            for i in 0..300 {
                tree.insert_with_key(keys.insert(()), rect(n + i));
            }
            for (j, (key, _)) in strokes.iter().enumerate() {
                tree.update_with_key(*key, rect(1000 + j as u64));
            }
            assert_eq!(tree.iter().count(), n as usize + 300);
            assert!(tree.is_balanced());
        }
    }

    #[test]
    fn non_finite_bounds_are_not_inserted() {
        let mut keys = SlotMap::<StrokeKey, ()>::with_key();
        let mut tree = KeyTree::default();
        let invalid = [
            Aabb::new(Vector2::new(f64::NAN, 0.0), Vector2::new(10.0, 10.0)),
            Aabb::new(Vector2::new(0.0, 0.0), Vector2::new(10.0, f64::INFINITY)),
            Aabb::new(
                Vector2::new(f64::NEG_INFINITY, 0.0),
                Vector2::new(10.0, 10.0),
            ),
        ];
        let mut invalid_keys = Vec::new();
        for i in 0..2000 {
            if i % 100 == 50 {
                let key = keys.insert(());
                tree.insert_with_key(key, invalid[(i / 100) % invalid.len()]);
                invalid_keys.push(key);
            } else {
                tree.insert_with_key(keys.insert(()), rect(i as u64));
            }
        }
        // Updating a stroke to invalid bounds removes it from the tree.
        let (key, _) = tree.iter().next().unwrap();
        tree.update_with_key(key, invalid[0]);
        invalid_keys.push(key);
        tree.rebuild_from_vec(vec![
            (keys.insert(()), invalid[1]),
            (keys.insert(()), rect(0)),
        ]);
        assert_eq!(tree.iter().count(), 1);
        assert!(
            tree.iter()
                .all(|(key, bounds)| !invalid_keys.contains(&key) && bounds_are_valid(&bounds))
        );
    }
}
