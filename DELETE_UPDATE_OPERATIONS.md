# Lance Delete and Update Operations Explained

This document explains how DELETE and UPDATE operations work in Lance, step by step, and how they modify the dataset structure.

---

## 1. DELETE Operation

### Overview
DELETE operations in Lance use **logical deletion** - rows are marked as deleted without rewriting the data files. This is efficient because it avoids rewriting large data files.

### Step-by-Step Process

#### Step 1: Identify Rows to Delete
```
User Query: dataset.delete("age > 65")
                │
                ▼
┌─────────────────────────────────────────┐
│ 1. Scan dataset with filter predicate   │
│    - Create scanner with filter         │
│    - Project only _rowid column         │
│    - Execute scan to get matching rows  │
└─────────────────────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────┐
│ 2. Collect Row Addresses                │
│    - Extract row IDs from scan results  │
│    - Convert row IDs → row addresses    │
│      (fragment_id, local_offset)        │
│    - Group by fragment_id               │
└─────────────────────────────────────────┘
```

#### Step 2: Apply Deletions to Fragments
```
For each fragment with deletions:
                │
                ▼
┌─────────────────────────────────────────┐
│ 3. Extend Deletion Vector              │
│    - Load existing deletion vector      │
│      (if fragment already has deletions)│
│    - Merge new deletions into vector    │
│    - Check if fragment is fully deleted│
└─────────────────────────────────────────┘
                │
                ▼
        ┌───────┴───────┐
        │               │
        ▼               ▼
┌──────────────┐  ┌──────────────┐
│ Partially    │  │ Fully        │
│ Deleted      │  │ Deleted      │
│              │  │              │
│ → Create/    │  │ → Mark       │
│   update     │  │   fragment   │
│   deletion   │  │   for        │
│   file       │  │   removal    │
└──────────────┘  └──────────────┘
```

#### Step 3: Write Deletion File
```
┌─────────────────────────────────────────┐
│ 4. Write Deletion File                  │
│    File: _deletions/{fragment_id}-{version}-{id}.{suffix}│
│    Example: _deletions/0-2-12345.arrow  │
│                                         │
│    Contents:                            │
│    - Local row offsets (within fragment)│
│    - Format: Array (.arrow) or Bitmap (.bin)│
│    - num_deleted_rows: usize           │
└─────────────────────────────────────────┘
```

#### Step 4: Update Fragment Metadata
```
┌─────────────────────────────────────────┐
│ 5. Update Fragment                      │
│    - Add deletion_file reference        │
│    - Update num_deleted_rows            │
│    - Keep all data files unchanged      │
└─────────────────────────────────────────┘
```

#### Step 5: Commit Transaction
```
┌─────────────────────────────────────────┐
│ 6. Create Transaction                  │
│    Operation::Delete {                  │
│      updated_fragments: [Fragment],     │
│      deleted_fragment_ids: [u64],       │
│      predicate: String                 │
│    }                                    │
└─────────────────────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────┐
│ 7. Write New Manifest                   │
│    - Update fragment list               │
│      • Modified fragments (with deletion_file)│
│      • Remove fully deleted fragments    │
│    - Increment version number           │
│    - Write _versions/{version}.manifest │
└─────────────────────────────────────────┘
```

### Example: DELETE Operation Flow

**Before DELETE:**
```
Fragment 0:
  - Data File: data/uuid1.lance (100 rows)
  - Deletion File: None
  
Fragment 1:
  - Data File: data/uuid2.lance (100 rows)
  - Deletion File: None
```

**DELETE Query:** `"id < 10 OR id >= 190"`

**After DELETE:**
```
Fragment 0:
  - Data File: data/uuid1.lance (100 rows) ← UNCHANGED
  - Deletion File: _deletions/0-2-12345.arrow ← NEW
    • Deleted local offsets: [0, 1, 2, ..., 9]
    • num_deleted_rows: 10

Fragment 1:
  - Data File: data/uuid2.lance (100 rows) ← UNCHANGED
  - Deletion File: _deletions/1-2-12346.bin ← NEW
    • Deleted local offsets: [90, 91, ..., 99]
    • num_deleted_rows: 10
```

**Key Points:**
- ✅ Data files are **NOT rewritten** - they remain unchanged
- ✅ New deletion files are created/updated
- ✅ Fragment metadata is updated to reference deletion file
- ✅ Manifest is updated with new version

---

## 2. UPDATE Operation

### Overview
UPDATE operations in Lance use a **copy-on-write** strategy:
1. Read matching rows
2. Apply updates in memory
3. Write updated rows as **new fragments**
4. Mark original rows as deleted in **old fragments**

### Step-by-Step Process

#### Step 1: Identify Rows to Update
```
User Query: dataset.update()
  .update_where("region_id = 10")
  .set("region_name", "New York")
                │
                ▼
┌─────────────────────────────────────────┐
│ 1. Scan dataset with filter predicate   │
│    - Create scanner with filter         │
│    - Include _rowid column              │
│    - Project all columns                │
│    - Execute scan to get matching rows  │
└─────────────────────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────┐
│ 2. Capture Row IDs                     │
│    - Extract row IDs from scan results  │
│    - Store for later deletion           │
└─────────────────────────────────────────┘
```

#### Step 2: Apply Updates to Data
```
┌─────────────────────────────────────────┐
│ 3. Apply Updates In-Memory             │
│    For each RecordBatch:                │
│    - Evaluate update expressions         │
│      (e.g., "region_name = 'New York'") │
│    - Replace column values               │
│    - Keep all other columns unchanged    │
└─────────────────────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────┐
│ 4. Write Updated Rows as New Fragments  │
│    - Create new data files              │
│      data/{new-uuid}.lance              │
│    - Write updated RecordBatches         │
│    - Assign new fragment IDs            │
│    - Set row_id_meta (preserve row IDs) │
└─────────────────────────────────────────┘
```

#### Step 3: Mark Original Rows as Deleted
```
┌─────────────────────────────────────────┐
│ 5. Apply Deletions to Old Fragments    │
│    - Convert row IDs → row addresses    │
│    - Group by fragment_id               │
│    - For each affected fragment:        │
│      • Load existing deletion vector    │
│      • Add new deletions                │
│      • Write/update deletion file       │
└─────────────────────────────────────────┘
```

#### Step 4: Commit Transaction
```
┌─────────────────────────────────────────┐
│ 6. Create Transaction                  │
│    Operation::Update {                  │
│      removed_fragment_ids: [u64],      │
│      updated_fragments: [Fragment],     │
│      new_fragments: [Fragment],        │
│      fields_modified: [u32],           │
│      update_mode: RewriteRows          │
│    }                                    │
└─────────────────────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────┐
│ 7. Write New Manifest                   │
│    - Add new fragments                  │
│    - Update old fragments (with deletions)│
│    - Remove fully deleted fragments    │
│    - Increment version number           │
│    - Write _versions/{version}.manifest │
└─────────────────────────────────────────┘
```

### Example: UPDATE Operation Flow

**Before UPDATE:**
```
Fragment 0:
  - Data File: data/uuid1.lance
    • Row 0: (id=5, region_id=10, region_name="California")
    • Row 1: (id=6, region_id=10, region_name="California")
    • Row 2: (id=7, region_id=20, region_name="Texas")
  - Deletion File: None
```

**UPDATE Query:** 
```rust
dataset.update()
  .update_where("region_id = 10")
  .set("region_name", "New York")
```

**After UPDATE:**
```
Fragment 0 (OLD - marked for deletion):
  - Data File: data/uuid1.lance ← UNCHANGED
    • Row 0: (id=5, region_id=10, region_name="California") ← DELETED
    • Row 1: (id=6, region_id=10, region_name="California") ← DELETED
    • Row 2: (id=7, region_id=20, region_name="Texas") ← KEPT
  - Deletion File: _deletions/0-2-12567.arrow ← NEW
    • Deleted local offsets: [0, 1]
    • num_deleted_rows: 2

Fragment 2 (NEW - contains updated rows):
  - Data File: data/uuid3.lance ← NEW
    • Row 0: (id=5, region_id=10, region_name="New York") ← UPDATED
    • Row 1: (id=6, region_id=10, region_name="New York") ← UPDATED
  - Deletion File: None
  - Row ID Meta: [5, 6] ← Preserves original row IDs
```

**Key Points:**
- ✅ Original data files are **NOT modified**
- ✅ Updated rows are written to **new fragments**
- ✅ Original rows are marked as deleted in **old fragments**
- ✅ Row IDs are preserved (stable row IDs)
- ✅ New manifest version is created

---

## 3. How Files Are Updated

### DELETE Operation File Changes

```
Before DELETE:
dataset/
├── _versions/
│   └── 1.manifest          (Fragment 0, Fragment 1)
├── data/
│   ├── uuid1.lance         (Fragment 0 data)
│   └── uuid2.lance         (Fragment 1 data)
└── _deletions/              (empty)

After DELETE:
dataset/
├── _versions/
│   ├── 1.manifest          (old version)
│   └── 2.manifest          ← NEW (Fragment 0 with deletion_file, Fragment 1 with deletion_file)
├── data/
│   ├── uuid1.lance         ← UNCHANGED
│   └── uuid2.lance         ← UNCHANGED
└── _deletions/
    ├── 0-2-12345.arrow     ← NEW (deletions for Fragment 0)
    └── 1-2-12346.bin       ← NEW (deletions for Fragment 1)
```

### UPDATE Operation File Changes

```
Before UPDATE:
dataset/
├── _versions/
│   └── 1.manifest          (Fragment 0)
├── data/
│   └── uuid1.lance         (Fragment 0 data)
└── _deletions/              (empty)

After UPDATE:
dataset/
├── _versions/
│   ├── 1.manifest          (old version)
│   └── 2.manifest          ← NEW (Fragment 0 with deletion_file, Fragment 2 NEW)
├── data/
│   ├── uuid1.lance         ← UNCHANGED (old data still exists)
│   └── uuid3.lance         ← NEW (updated rows)
└── _deletions/
    └── 0-2-12567.arrow     ← NEW (deletions for Fragment 0)
```

---

## 4. Fragment Metadata Changes

### DELETE Operation Fragment Changes

**Fragment Before:**
```rust
Fragment {
    id: 0,
    files: [DataFile { path: "data/uuid1.lance", ... }],
    deletion_file: None,                    // ← No deletions
    physical_rows: Some(100),
    row_id_meta: Some(...),
}
```

**Fragment After:**
```rust
Fragment {
    id: 0,
    files: [DataFile { path: "data/uuid1.lance", ... }],  // ← UNCHANGED
    deletion_file: Some(DeletionFile {                    // ← NEW
        read_version: 2,
        id: 123,
        file_type: Bitmap,
        num_deleted_rows: Some(10),
        ...
    }),
    physical_rows: Some(100),              // ← UNCHANGED
    row_id_meta: Some(...),                // ← UNCHANGED
}
```

### UPDATE Operation Fragment Changes

**Old Fragment (marked for deletion):**
```rust
Fragment {
    id: 0,
    files: [DataFile { path: "data/uuid1.lance", ... }],  // ← UNCHANGED
    deletion_file: Some(DeletionFile {                    // ← NEW
        read_version: 2,
        id: 125,
        file_type: Bitmap,
        num_deleted_rows: Some(2),         // ← Rows that were updated
        ...
    }),
    physical_rows: Some(100),
    row_id_meta: Some(...),
}
```

**New Fragment (with updated data):**
```rust
Fragment {
    id: 2,                                  // ← NEW fragment ID
    files: [DataFile { path: "data/uuid3.lance", ... }], // ← NEW file
    deletion_file: None,
    physical_rows: Some(2),                 // ← Only updated rows
    row_id_meta: Some(RowIdMeta::Inline([5, 6])), // ← Preserves original row IDs
}
```

---

## 5. Key Differences: DELETE vs UPDATE

| Aspect                  | DELETE                    | UPDATE                             |
| ----------------------- | ------------------------- | ---------------------------------- |
| **Data Files**          | Unchanged                 | New files created for updated rows |
| **Deletion Files**      | Created/updated           | Created/updated for old fragments  |
| **New Fragments**       | None                      | Yes (one or more)                  |
| **Fragment Removal**    | If fully deleted          | If fully deleted                   |
| **Row ID Preservation** | N/A                       | Yes (via row_id_meta)              |
| **Write Cost**          | Low (only deletion files) | Higher (new data files)            |
| **Read Cost**           | Filter deleted rows       | Read from new fragments            |

---

## 6. Important Concepts

### Logical Deletion
- Rows are marked as deleted, not physically removed
- Data files remain unchanged (efficient!)
- Deletion files track which rows are deleted
- Readers filter out deleted rows automatically

### Copy-on-Write for Updates
- Original data is never modified
- Updated rows are written to new fragments
- Old rows are marked as deleted
- Enables time travel (can read old versions)

### Fragment Lifecycle
1. **Created**: New fragment with data files
2. **Modified**: Deletion file added/updated
3. **Removed**: If all rows are deleted (or after compaction)

### Versioning
- Each operation creates a new manifest version
- Old versions remain accessible (time travel)
- Version number increments: 1 → 2 → 3 → ...
- Transaction file manages atomic commits

---

## 7. Performance Considerations

### DELETE Operations
- ✅ **Fast**: Only writes small deletion files
- ✅ **Efficient**: Doesn't rewrite large data files
- ⚠️ **Accumulation**: Deletion files accumulate over time
- 💡 **Solution**: Compaction merges fragments and removes deletions

### UPDATE Operations
- ⚠️ **Slower**: Creates new data files
- ⚠️ **Storage**: Increases storage (old + new data)
- ✅ **Efficient reads**: New fragments are optimized
- 💡 **Solution**: Compaction merges fragments and removes old data

### Compaction
- Merges fragments together
- Removes deleted rows physically
- Consolidates deletion files
- Reduces storage and improves read performance

---

## 8. Where Do Deletion Files Live?

### Physical Storage
Deletion files are stored **separately** from data files in the `_deletions/` directory:

```
dataset/
├── data/
│   └── {uuid}.lance          ← Data files (fragment data)
└── _deletions/
    └── {fragment_id}-{version}-{id}.{suffix}  ← Deletion files
```

**File naming pattern:**
- Format: `{fragment_id}-{read_version}-{id}.{suffix}`
- Suffix: `.arrow` (Array format) or `.bin` (Bitmap format)
- Example: `0-2-12345.arrow` (Fragment 0, version 2, ID 12345, Array format)

### Logical Association
Deletion files are **logically part of the fragment** through the Fragment metadata:

```rust
pub struct Fragment {
    pub id: u64,
    pub files: Vec<DataFile>,              // Data files
    pub deletion_file: Option<DeletionFile>, // ← Reference to deletion file
    // ...
}

pub struct DeletionFile {
    pub read_version: u64,      // Version when deletion was written
    pub id: u64,                // Unique ID for this deletion file
    pub file_type: DeletionFileType,  // Array or Bitmap
    pub num_deleted_rows: Option<usize>,
    pub base_id: Option<u32>,
}
```

### How It Works
1. **Storage**: Deletion files are stored in `_deletions/` directory (separate from `data/`)
2. **Reference**: Fragment contains `DeletionFile` metadata (not the file path)
3. **Path Construction**: Path is built dynamically using:
   - Fragment ID (from fragment)
   - Read version (from DeletionFile)
   - ID (from DeletionFile)
   - File type suffix (from DeletionFile)
4. **Manifest**: Fragment metadata (including deletion_file reference) is stored in manifest

### Key Points
- ✅ **Physically separate**: Deletion files live in `_deletions/`, not inside data files
- ✅ **Logically part of fragment**: Fragment metadata references the deletion file
- ✅ **Not embedded**: Deletion files are separate files, not embedded in `.lance` data files
- ✅ **Referenced in manifest**: Fragment entry in manifest includes deletion_file metadata

### Example Structure

```
Fragment 0 (in manifest):
  id: 0
  files: [DataFile { path: "data/uuid1.lance" }]
  deletion_file: Some(DeletionFile {
    read_version: 2,
    id: 12345,
    file_type: Bitmap,
    num_deleted_rows: Some(10)
  })

Physical files:
  data/uuid1.lance                    ← Data file (unchanged)
  _deletions/0-2-12345.bin           ← Deletion file (separate file)
```

---

## Summary

**DELETE:**
1. Scan and identify rows to delete
2. Create/update deletion files per fragment (in `_deletions/` directory)
3. Update fragment metadata (add deletion_file reference)
4. Commit new manifest version

**UPDATE:**
1. Scan and identify rows to update
2. Apply updates in memory
3. Write updated rows to new fragments
4. Mark original rows as deleted in old fragments (create deletion files)
5. Commit new manifest version

Both operations preserve data files and use logical deletion, enabling efficient updates and time travel capabilities.

**Deletion Files:**
- Stored in `_deletions/` directory (physically separate)
- Referenced in Fragment metadata (logically part of fragment)
- Path constructed dynamically from fragment ID and deletion file metadata

---

## 9. How Does Lance Know Which Fragment Has the Latest Data?

After an UPDATE operation, you might have:
- **Fragment 0**: Old data with rows marked as deleted
- **Fragment 2**: New data with updated rows

**Question**: How does Lance know that Fragment 2 contains the latest version of rows that were in Fragment 0?

### Answer: Row IDs + Row ID Index

Lance doesn't explicitly track that Fragment 0 and Fragment 2 are "related". Instead, it uses **Row IDs** and a **Row ID Index** to find the correct fragment.

### How It Works

#### 1. Row IDs Are Preserved Across Updates

When rows are updated, the **new fragment preserves the original row IDs**:

```rust
// Fragment 0 (OLD)
row_id_meta: Some([5, 6, 7, ...])  // Original row IDs

// Fragment 2 (NEW - after update)
row_id_meta: Some([5, 6])          // Same row IDs preserved!
```

The UPDATE operation explicitly preserves row IDs (see line 328-347 in update.rs):
- Captures row IDs from old rows
- Assigns same row IDs to new fragment via `row_id_meta`

#### 2. Row ID Index Maps Row IDs → Row Addresses

The **Row ID Index** is built from all fragments and maps:
- **Row ID** → **Row Address** (fragment_id + local_offset)

```rust
RowIdIndex {
    // Maps row_id ranges to (row_id_segment, address_segment)
    // Example:
    row_id: 5 → RowAddress { fragment_id: 2, offset: 0 }
    row_id: 6 → RowAddress { fragment_id: 2, offset: 1 }
    row_id: 7 → RowAddress { fragment_id: 0, offset: 2 }  // Not updated
}
```

#### 3. Deletion Vectors Exclude Old Rows

When building the Row ID Index:
- **Fragment 0**: Rows 5 and 6 are marked as deleted → **excluded from index**
- **Fragment 2**: Rows 5 and 6 are active → **included in index**

```rust
// Building Row ID Index
FragmentRowIdIndex {
    fragment_id: 0,
    row_id_sequence: [5, 6, 7, ...],
    deletion_vector: [0, 1],  // ← Rows 5 and 6 are deleted
}
// Result: Only row 7 (offset 2) is included in index

FragmentRowIdIndex {
    fragment_id: 2,
    row_id_sequence: [5, 6],
    deletion_vector: [],  // ← No deletions
}
// Result: Rows 5 and 6 are included in index
```

#### 4. Reading Data Uses Row ID Index

When reading a row by row ID:

```
1. Query: "Get row with row_id = 5"
   │
   ▼
2. Lookup in Row ID Index:
   row_id: 5 → RowAddress { fragment_id: 2, offset: 0 }
   │
   ▼
3. Read from Fragment 2, offset 0
   ✅ Gets updated data
```

### Example Flow

**Before UPDATE:**
```
Fragment 0:
  row_id_meta: [5, 6, 7]
  data: [old_data_5, old_data_6, old_data_7]
  deletion_file: None

Row ID Index:
  5 → Fragment 0, offset 0
  6 → Fragment 0, offset 1
  7 → Fragment 0, offset 2
```

**After UPDATE (rows 5 and 6):**
```
Fragment 0 (OLD):
  row_id_meta: [5, 6, 7]
  data: [old_data_5, old_data_6, old_data_7]  ← UNCHANGED
  deletion_file: Some(...)  ← Rows 5, 6 marked deleted

Fragment 2 (NEW):
  row_id_meta: [5, 6]  ← Same row IDs preserved!
  data: [new_data_5, new_data_6]  ← Updated data
  deletion_file: None

Row ID Index (rebuilt):
  5 → Fragment 2, offset 0  ← Points to NEW fragment
  6 → Fragment 2, offset 1  ← Points to NEW fragment
  7 → Fragment 0, offset 2  ← Still in old fragment (not updated)
```

**Key Points:**
- ✅ Row IDs are **stable identifiers** - they don't change when rows are updated
- ✅ New fragments **preserve row IDs** via `row_id_meta`
- ✅ Old fragments **mark rows as deleted** via deletion files
- ✅ Row ID Index **excludes deleted rows** from old fragments
- ✅ Row ID Index **includes active rows** from new fragments
- ✅ Reading by row ID automatically finds the **latest version**

### Why This Works

1. **No explicit relationship tracking**: Lance doesn't need to track that Fragment 0 and Fragment 2 are "related"
2. **Row IDs are the link**: Same row IDs in both fragments create the logical connection
3. **Deletion vectors filter**: Old fragments exclude updated rows from the index
4. **Index resolves conflicts**: If same row ID exists in multiple fragments, deletion vectors ensure only the active one is indexed

### Summary

**The system doesn't explicitly "know" Fragment 0 and Fragment 2 are related. Instead:**

1. **Row IDs** provide the logical link (same IDs in both fragments)
2. **Deletion vectors** mark old rows as inactive
3. **Row ID Index** maps row IDs to the correct fragment (excluding deleted rows)
4. **Readers** use the index to find the latest version automatically

This design is elegant because:
- ✅ No explicit relationship tracking needed
- ✅ Works naturally with multiple updates
- ✅ Handles concurrent updates correctly
- ✅ Enables time travel (can read old versions by using old manifest)
