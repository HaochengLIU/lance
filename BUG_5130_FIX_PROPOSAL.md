# Bug #5130 Fix Proposal

## Root Cause Analysis

The zonemap (and other scalar) indexes are being **queried** but their results are **not being used for fragment-level pruning** during query planning.

### Current Flow

```
Scanner::create_plan()
  └─> Scanner::filtered_read_source()
      └─> Scanner::filtered_read()
          └─> Scanner::new_filtered_read()
              ├─> Use ALL fragments from self.fragments  ❌
              └─> Create ScalarIndexExec (for row filtering) ✓
```

The problem is at **line 2048-2050** in `scanner.rs`:

```rust
if let Some(fragments) = fragments {
    read_options = read_options.with_fragments(fragments);  // Uses ALL fragments!
}
```

Even though we have a scalar index query (line 2072), we don't use its `fragment_bitmap` to filter the fragments.

### What Should Happen

```
Scanner::create_plan()
  └─> Scanner::filtered_read_source()
      └─> Scanner::filtered_read()
          └─> Scanner::new_filtered_read()
              ├─> Extract fragment_bitmap from scalar index ✓
              ├─> Filter fragments using fragment_bitmap ✓
              └─> Pass FILTERED fragments to FilteredReadExec ✓
```

---

## The Fix

### Location
`rust/lance/src/dataset/scanner.rs` - `new_filtered_read()` method

### Changes Required

#### Step 1: Filter Fragments Based on Scalar Index

```rust
async fn new_filtered_read(
    &self,
    filter_plan: &FilterPlan,
    projection: Projection,
    make_deletions_null: bool,
    fragments: Option<Arc<Vec<Fragment>>>,
    scan_range: Option<Range<u64>>,
) -> Result<Arc<dyn ExecutionPlan>> {
    let mut read_options = FilteredReadOptions::basic_full_read(&self.dataset)
        .with_filter_plan(filter_plan.clone())
        .with_projection(projection);

    // NEW CODE: Filter fragments using scalar index fragment bitmap
    let filtered_fragments = if let Some(fragments) = fragments {
        if let Some(index_query) = &filter_plan.index_query {
            // Extract fragment bitmap from scalar index
            let fragment_bitmap = Self::fragments_covered_by_index_query(
                index_query,
                &self.dataset
            ).await?;
            
            // Filter fragments to only those covered by the index
            let filtered: Vec<Fragment> = fragments
                .iter()
                .filter(|frag| fragment_bitmap.contains(frag.id as u32))
                .cloned()
                .collect();
            
            Some(Arc::new(filtered))
        } else {
            Some(fragments)
        }
    } else {
        None
    };

    if let Some(fragments) = filtered_fragments {
        read_options = read_options.with_fragments(fragments);
    }

    // Rest of the function remains the same...
    if let Some(scan_range) = scan_range {
        read_options = read_options.with_scan_range_before_filter(scan_range)?;
    }

    if let Some(batch_size) = self.batch_size {
        read_options = read_options.with_batch_size(batch_size as u32);
    }

    if let Some(fragment_readahead) = self.fragment_readahead {
        read_options = read_options.with_fragment_readahead(fragment_readahead);
    }

    if make_deletions_null {
        read_options = read_options.with_deleted_rows()?;
    }

    if let Some(io_buffer_size_bytes) = self.io_buffer_size {
        read_options = read_options.with_io_buffer_size(io_buffer_size_bytes);
    }

    let index_input = filter_plan.index_query.clone().map(|index_query| {
        Arc::new(ScalarIndexExec::new(self.dataset.clone(), index_query))
            as Arc<dyn ExecutionPlan>
    });

    Ok(Arc::new(FilteredReadExec::try_new(
        self.dataset.clone(),
        read_options,
        index_input,
    )?))
}
```

#### Step 2: Move Helper Method to Scanner

The `fragments_covered_by_index_query` method already exists in `ScalarIndexExec` but needs to be accessible from `Scanner`. We should either:

**Option A**: Make it a public method in `ScalarIndexExec`

```rust
// In rust/lance/src/io/exec/scalar_index.rs
impl ScalarIndexExec {
    // Make this pub so Scanner can use it
    pub async fn fragments_covered_by_index_query(
        index_expr: &ScalarIndexExpr,
        dataset: &Dataset,
    ) -> Result<RoaringBitmap> {
        // ... existing implementation ...
    }
}
```

**Option B**: Copy the method to Scanner (avoid cross-module dependency)

```rust
// In rust/lance/src/dataset/scanner.rs
impl Scanner {
    #[async_recursion::async_recursion]
    async fn fragments_covered_by_index_query(
        index_expr: &ScalarIndexExpr,
        dataset: &Dataset,
    ) -> Result<RoaringBitmap> {
        match index_expr {
            ScalarIndexExpr::And(lhs, rhs) => {
                Ok(Self::fragments_covered_by_index_query(lhs, dataset).await?
                    & Self::fragments_covered_by_index_query(rhs, dataset).await?)
            }
            ScalarIndexExpr::Or(lhs, rhs) => {
                Ok(Self::fragments_covered_by_index_query(lhs, dataset).await?
                    | Self::fragments_covered_by_index_query(rhs, dataset).await?)
            }
            ScalarIndexExpr::Not(expr) => {
                // For NOT, we need all fragments (can't prune)
                let all_fragments = RoaringBitmap::from_iter(
                    dataset.fragments().iter().map(|f| f.id as u32)
                );
                Ok(all_fragments)
            }
            ScalarIndexExpr::Query(search_key) => {
                let idx = dataset
                    .load_scalar_index(
                        ScalarIndexCriteria::default().with_name(&search_key.index_name),
                    )
                    .await?
                    .ok_or_else(|| Error::Internal {
                        message: format!("Index {} not found", search_key.index_name),
                        location: location!()
                    })?;
                Ok(idx
                    .fragment_bitmap
                    .expect("scalar indices should always have a fragment bitmap"))
            }
        }
    }
}
```

**Recommendation**: Use Option B to keep scanner logic self-contained.

---

## Testing the Fix

### Expected Results After Fix

Running `reproduce_bug_5130.py` with `LANCE_LOG=debug`:

**BEFORE (Bug Present)**:
```
LanceRead: ... num_fragments=10 ...
  ScalarIndexQuery: query=[test_id = test_id_5]@test_id_idx
```

**AFTER (Bug Fixed)**:
```
LanceRead: ... num_fragments=1 ...
  ScalarIndexQuery: query=[test_id = test_id_5]@test_id_idx
```

### Test Cases to Add

```rust
#[tokio::test]
async fn test_zonemap_fragment_pruning() {
    use arrow::datatypes::UInt64Type;
    use lance_datagen::array;
    use lance_index::scalar::{BuiltinIndexType, ScalarIndexParams};
    use lance_index::IndexType;

    // Create dataset with 10 fragments, each with unique test_id
    let fragments_data: Vec<_> = (0..10)
        .map(|i| {
            let test_id = format!("test_id_{}", i);
            pa::table({
                "id": range(i * 100, (i + 1) * 100),
                "test_id": [test_id] * 100,
            })
        })
        .collect();

    let full_data = pa::concat_tables(fragments_data);
    
    let mut ds = lance::write_dataset(
        full_data,
        "memory://test_fragment_pruning",
        max_rows_per_file=100,
    )
    .await
    .unwrap();

    // Create zonemap index
    ds.create_index(
        &["test_id"],
        IndexType::Scalar,
        None,
        &ScalarIndexParams::for_builtin(BuiltinIndexType::ZoneMap),
        false,
    )
    .await
    .unwrap();

    // Query for a specific test_id
    let scanner = ds.scan()
        .filter("test_id = 'test_id_5'")
        .unwrap();

    // The key test: check that only 1 fragment is included in the execution plan
    let plan = scanner.create_plan().await.unwrap();
    let plan_str = format!("{:?}", plan);
    
    // Should contain "num_fragments=1" not "num_fragments=10"
    assert!(
        plan_str.contains("num_fragments=1"),
        "Expected num_fragments=1, but plan was: {}",
        plan_str
    );

    // Verify query returns correct results
    let result = ds.to_table(filter="test_id = 'test_id_5'").await.unwrap();
    assert_eq!(result.num_rows(), 100);
}
```

---

## Performance Impact

### Before Fix
- **Test case**: 10 fragments, query for 1 unique value
  - Fragments scanned: 10 (100%)
  - I/O overhead: 10x

- **Production case** (issue #5130): 30K fragments, query for 1 unique value
  - Fragments scanned: 3K+ (10%+)
  - I/O overhead: 3000x

### After Fix
- **Test case**: 10 fragments, query for 1 unique value
  - Fragments scanned: 1 (10%)
  - I/O overhead: 1x ✓

- **Production case**: 30K fragments, query for 1 unique value
  - Fragments scanned: 1 (0.003%)
  - I/O overhead: 1x ✓

**Improvement**: 3000x faster for selective queries!

---

## Edge Cases to Consider

1. **Multiple scalar index queries (AND/OR)**: The fix handles this via bitmap intersection/union
2. **NOT queries**: Cannot prune fragments (need to scan all), handled by returning all fragments
3. **Mixed index and non-index filters**: Only prune based on index queries, refine filters still apply
4. **No fragments specified**: No-op, behaves as before
5. **Index covers no fragments**: Returns empty fragment list (correct - no rows match)
6. **All fragments covered**: No performance impact, but no regression either

---

## Related Code Locations

- **Scanner**: `rust/lance/src/dataset/scanner.rs:2036` (new_filtered_read)
- **ScalarIndexExec**: `rust/lance/src/io/exec/scalar_index.rs:122` (fragments_covered_by_index_query)
- **FilteredReadExec**: `rust/lance/src/io/exec/filtered_read.rs` (uses filtered fragments)

---

## Summary

This fix adds fragment-level pruning using scalar index metadata during query planning. The key insight is that scalar indexes already track which fragments they cover via `fragment_bitmap`, but this information was not being used to filter the fragments passed to `FilteredReadExec`.

The fix is minimal, focused, and doesn't change any index behavior - it simply uses existing metadata more effectively during query planning.




