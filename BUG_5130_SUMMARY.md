# Bug #5130: Zonemap Index Scanning Too Many Fragments

## 📋 Summary

**Issue**: Zonemap (and other scalar) indexes query the index but don't use the results to prune fragments at the query planning stage, resulting in scanning all fragments instead of only relevant ones.

**Impact**: 3000x performance overhead in production (30K fragments → 3K+ scanned instead of 1)

**Root Cause**: Scanner passes all fragments to FilteredReadExec without filtering based on scalar index metadata

**Fix**: Use scalar index's `fragment_bitmap` to filter fragments before execution

---

## 📁 Files

1. **`reproduce_bug_5130.py`** - Standalone Python script to reproduce the bug
   - Creates 10 fragments with unique test_ids
   - Demonstrates num_fragments=10 when it should be 1

2. **`BUG_5130_REPRODUCTION.md`** - Detailed bug reproduction analysis
   - Execution plan comparison
   - Root cause explanation
   - Impact assessment

3. **`BUG_5130_FIX_PROPOSAL.md`** - Comprehensive fix with code changes
   - Exact code modifications needed
   - Test cases to add
   - Performance impact analysis

4. **`BUG_5130_FLOW_DIAGRAM.md`** - Visual flow diagrams
   - BEFORE/AFTER comparison
   - Key insights
   - Verification steps

---

## 🚀 Quick Start

### Reproduce the Bug

```bash
# Run the reproduction script
cd /Users/haochengliu/Documents/projects/lance
source .venv/bin/activate
python reproduce_bug_5130.py

# See the execution plan with bug
LANCE_LOG=debug python reproduce_bug_5130.py 2>&1 | grep -B 1 "ScalarIndexQuery"
```

Expected output showing the bug:
```
LanceRead: ... num_fragments=10 ...  ← BUG!
  ScalarIndexQuery: query=[test_id = test_id_5]@test_id_idx
```

### Apply the Fix

1. **Edit**: `rust/lance/src/dataset/scanner.rs` (line ~2036)
2. **Add**: Fragment filtering logic in `new_filtered_read()` method
3. **Test**: Run `cargo test` and `python reproduce_bug_5130.py`
4. **Verify**: `num_fragments=1` in execution plan

See `BUG_5130_FIX_PROPOSAL.md` for exact code changes.

---

## 🔍 Key Code Locations

```
rust/lance/src/dataset/scanner.rs
├─ Line 2036: new_filtered_read()
│  └─ Line 2048-2050: ❌ BUG HERE - uses all fragments
│  └─ Line 2072: ✓ Creates ScalarIndexExec
│  └─ FIX: Add fragment filtering between these lines
│
rust/lance/src/io/exec/scalar_index.rs
├─ Line 122: fragments_covered_by_index_query()
│  └─ Already extracts fragment_bitmap from index
│  └─ Need to make this accessible to Scanner
│
rust/lance/src/io/exec/filtered_read.rs
└─ FilteredReadExec receives filtered fragments
```

---

## 📊 Performance Impact

| Scenario | Before Fix | After Fix | Improvement |
|----------|-----------|-----------|-------------|
| Test (10 fragments) | 10 scanned | 1 scanned | 10x faster |
| Production (30K fragments) | 3K+ scanned | 1 scanned | 3000x faster |

---

## 🔗 Related Issues

- **#5130**: Zonemap index reading too many files (THIS BUG)
- **#4758**: Zonemap with deletions (FIXED - separate issue)

These are **different bugs**:
- #4758: Zone length calculation wrong with deletions
- #5130: Fragment-level pruning not working at all

---

## ✅ Verification Checklist

After applying the fix:

- [ ] `cargo build` succeeds
- [ ] `cargo test -p lance` passes
- [ ] Run `reproduce_bug_5130.py` shows `num_fragments=1`
- [ ] Add test case in `rust/lance/src/dataset/scanner.rs`
- [ ] Update PR description with before/after execution plans

---

## 🎯 Next Steps

1. Review `BUG_5130_FIX_PROPOSAL.md` for implementation details
2. Apply code changes to `scanner.rs`
3. Add test case for fragment pruning
4. Run reproduction script to verify fix
5. Submit PR with before/after logs

---

## 📞 Contact

Created for: @HaochengLIU
Issue: https://github.com/lancedb/lance/issues/5130
Date: November 6, 2025
