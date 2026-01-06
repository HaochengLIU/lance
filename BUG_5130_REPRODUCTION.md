# Bug #5130 Reproduction Results

## 🐛 Bug Confirmed: Zonemap Index Scanning Too Many Fragments

### Test Setup
- **Dataset**: 10 fragments, 100 rows each
- **Key characteristic**: Each fragment contains UNIQUE test_id values
  - Fragment 0: test_id_0 (rows 0-99)
  - Fragment 1: test_id_1 (rows 100-199)
  - ... 
  - Fragment 5: test_id_5 (rows 500-599)
  - ...
  - Fragment 9: test_id_9 (rows 900-999)

### Query
```sql
test_id = 'test_id_5'
```

**Expected behavior**: Should scan ONLY fragment 5 (1 fragment)
**Actual behavior**: Scans ALL 10 fragments

---

## 📊 Execution Plan Analysis

### WITHOUT Zonemap Index (Baseline)
```
LanceRead: num_fragments=10, full_filter=test_id = Utf8("test_id_5"), refine_filter=test_id = Utf8("test_id_5")
```
✓ As expected: Full table scan of all 10 fragments with filter applied

### WITH Zonemap Index (BUG)
```
LanceRead: num_fragments=10, full_filter=test_id = Utf8("test_id_5"), refine_filter=--
  ScalarIndexQuery: query=[test_id = test_id_5]@test_id_idx
```
⚠️  **BUG**: Still scanning 10 fragments even with zonemap index!

The `ScalarIndexQuery` shows the zonemap index IS being used, but it's NOT filtering out fragments.

---

## 🔍 Root Cause Analysis

The zonemap index stores min/max statistics for each zone (row group) within fragments:
- Fragment 0, Zone 0-9: min="test_id_0", max="test_id_0"
- Fragment 5, Zone 0-9: min="test_id_5", max="test_id_5"
- etc.

When querying for `test_id = 'test_id_5'`:
1. ✓ Zonemap index is loaded
2. ✓ Query is matched against zonemap statistics
3. ❌ **PROBLEM**: The index is not properly filtering fragments at the query planning stage
4. ❌ Result: All 10 fragments are still included in the execution plan

---

## 💥 Impact

In the production case from issue #5130:
- 30,000 fragments total
- Each fragment has one unique test_id
- Query for single test_id scans **3,000+ fragments** instead of 1
- **Performance overhead**: 3000x slower than it should be!

---

## ✅ Query Correctness

The query DOES return correct results (100 rows with test_id_5), but at a massive performance cost due to scanning unnecessary fragments.

---

## 🔗 Related Bugs

- **Bug #4758**: Zonemap with deletions (FIXED in current PR)
- **Bug #5130**: Zonemap not filtering fragments (THIS BUG - still present)

These are separate issues:
- #4758: Zone length calculation was wrong when deletions created gaps in row addresses
- #5130: Fragment-level pruning is not working even without deletions




