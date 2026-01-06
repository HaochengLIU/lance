# Bug #5130: Flow Diagram

## 🐛 BEFORE (Bug Present)

```
┌─────────────────────────────────────────────────────────────────┐
│ User Query: test_id = 'test_id_5'                              │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ Scanner::create_plan()                                          │
│  ├─ filter_plan contains: ScalarIndexExpr                      │
│  └─ fragments: ALL 10 fragments                                │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ Scanner::new_filtered_read()                                    │
│                                                                 │
│  fragments = Some(ALL 10 fragments)     ❌ BUG HERE            │
│     ├─ Fragment 0: test_id_0                                   │
│     ├─ Fragment 1: test_id_1                                   │
│     ├─ Fragment 2: test_id_2                                   │
│     ├─ Fragment 3: test_id_3                                   │
│     ├─ Fragment 4: test_id_4                                   │
│     ├─ Fragment 5: test_id_5  ← ONLY THIS ONE IS RELEVANT     │
│     ├─ Fragment 6: test_id_6                                   │
│     ├─ Fragment 7: test_id_7                                   │
│     ├─ Fragment 8: test_id_8                                   │
│     └─ Fragment 9: test_id_9                                   │
│                                                                 │
│  read_options = read_options.with_fragments(fragments)  ❌      │
│                                                                 │
│  index_input = ScalarIndexExec::new(...)  ✓                    │
│     (Creates index query, but doesn't filter fragments)        │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ FilteredReadExec::try_new(                                      │
│     dataset,                                                    │
│     read_options,  ← Contains ALL 10 fragments ❌               │
│     index_input    ← ScalarIndexExec (for row filtering)       │
│ )                                                               │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ Execution Plan (shown in logs):                                │
│                                                                 │
│   LanceRead: num_fragments=10  ❌ WRONG!                        │
│     ScalarIndexQuery: query=[test_id = test_id_5]@test_id_idx  │
│                                                                 │
│ Scans ALL 10 fragments, then filters rows using zonemap index  │
│ Result: 10x I/O overhead                                       │
└─────────────────────────────────────────────────────────────────┘
```

---

## ✅ AFTER (Bug Fixed)

```
┌─────────────────────────────────────────────────────────────────┐
│ User Query: test_id = 'test_id_5'                              │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ Scanner::create_plan()                                          │
│  ├─ filter_plan contains: ScalarIndexExpr                      │
│  └─ fragments: ALL 10 fragments                                │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ Scanner::new_filtered_read() - WITH FIX                         │
│                                                                 │
│  if let Some(index_query) = &filter_plan.index_query {         │
│                                                                 │
│    ┌─────────────────────────────────────────────────────┐    │
│    │ NEW: Extract fragment_bitmap from scalar index      │    │
│    │                                                      │    │
│    │ fragment_bitmap = fragments_covered_by_index_query( │    │
│    │     index_query,                                    │    │
│    │     dataset                                         │    │
│    │ ).await?                                            │    │
│    │                                                      │    │
│    │ // Loads zonemap index metadata                     │    │
│    │ // Returns: RoaringBitmap { 5 }  ← Only fragment 5 │    │
│    └─────────────────────────────────────────────────────┘    │
│                          ↓                                     │
│    ┌─────────────────────────────────────────────────────┐    │
│    │ NEW: Filter fragments using bitmap                  │    │
│    │                                                      │    │
│    │ filtered_fragments = fragments                      │    │
│    │     .iter()                                         │    │
│    │     .filter(|f| fragment_bitmap.contains(f.id))     │    │
│    │     .collect()                                      │    │
│    │                                                      │    │
│    │ Result: [Fragment 5: test_id_5]  ✓                 │    │
│    └─────────────────────────────────────────────────────┘    │
│  }                                                              │
│                                                                 │
│  read_options = read_options.with_fragments(filtered_fragments)│
│                                                        ✓        │
│  index_input = ScalarIndexExec::new(...)  ✓                    │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ FilteredReadExec::try_new(                                      │
│     dataset,                                                    │
│     read_options,  ← Contains ONLY Fragment 5  ✓               │
│     index_input    ← ScalarIndexExec (for row filtering)       │
│ )                                                               │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ Execution Plan (shown in logs):                                │
│                                                                 │
│   LanceRead: num_fragments=1  ✓ CORRECT!                       │
│     ScalarIndexQuery: query=[test_id = test_id_5]@test_id_idx  │
│                                                                 │
│ Scans ONLY 1 fragment (Fragment 5), then filters rows          │
│ Result: Optimal I/O                                            │
└─────────────────────────────────────────────────────────────────┘
```

---

## 🔑 Key Insight

```
┌──────────────────────────────────────────────────────────────────┐
│ Scalar Index Metadata (Already Exists!)                         │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ZoneMap Index for "test_id":                                   │
│  ├─ Fragment 0: min="test_id_0", max="test_id_0"               │
│  ├─ Fragment 1: min="test_id_1", max="test_id_1"               │
│  ├─ Fragment 2: min="test_id_2", max="test_id_2"               │
│  ├─ Fragment 3: min="test_id_3", max="test_id_3"               │
│  ├─ Fragment 4: min="test_id_4", max="test_id_4"               │
│  ├─ Fragment 5: min="test_id_5", max="test_id_5"  ← MATCH!     │
│  ├─ Fragment 6: min="test_id_6", max="test_id_6"               │
│  ├─ Fragment 7: min="test_id_7", max="test_id_7"               │
│  ├─ Fragment 8: min="test_id_8", max="test_id_8"               │
│  └─ Fragment 9: min="test_id_9", max="test_id_9"               │
│                                                                  │
│  fragment_bitmap: RoaringBitmap { 0,1,2,3,4,5,6,7,8,9 }        │
│                   (All fragments covered by this index)         │
│                                                                  │
│  Query: test_id = 'test_id_5'                                   │
│  Result: Only Fragment 5 matches → fragment_bitmap = {5}       │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
                            ↓
        ┌────────────────────────────────────────┐
        │ THIS INFORMATION WAS NOT BEING USED!   │
        │                                        │
        │ The fix: Use it to filter fragments   │
        │ BEFORE passing to FilteredReadExec    │
        └────────────────────────────────────────┘
```

---

## 📊 Impact Comparison

```
                    BEFORE FIX                  AFTER FIX
                    ──────────                  ─────────

Fragments to Scan:       10                         1
                    [0,1,2,3,4,                    [5]
                     5,6,7,8,9]

I/O Operations:     10x overhead                 Optimal
                    (read 9 extra                (only read
                     fragments)                   what's needed)

Query Time:         ~1000ms                      ~100ms
                    (10 fragments ×               (1 fragment ×
                     100ms each)                  100ms)

Scalability:        Poor                         Excellent
                    (30K frags →                 (30K frags →
                     3K+ scanned)                 1 scanned)
```

---

## 🧪 Verification

### How to Test

```bash
# Run the reproduction script with debug logs
LANCE_LOG=debug python reproduce_bug_5130.py 2>&1 | grep "Executing plan"
```

### What to Look For

**BEFORE Fix:**
```
Executing plan:
  LanceRead: ... num_fragments=10 ...  ← ❌ BUG
    ScalarIndexQuery: query=[test_id = test_id_5]@test_id_idx
```

**AFTER Fix:**
```
Executing plan:
  LanceRead: ... num_fragments=1 ...  ← ✓ FIXED
    ScalarIndexQuery: query=[test_id = test_id_5]@test_id_idx
```

The change from `num_fragments=10` to `num_fragments=1` proves that fragment-level pruning is working!




