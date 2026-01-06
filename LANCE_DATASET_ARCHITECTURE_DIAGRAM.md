# Lance Dataset Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          LANCE DATASET ARCHITECTURE                          │
│                                                                               │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                         MANIFEST                                     │   │
│  │  ┌───────────────────────────────────────────────────────────────┐  │   │
│  │  │ Schema Fields:                                                │  │   │
│  │  │   • col1: string                                              │  │   │
│  │  │   • col2: string                                               │  │   │
│  │  │   • col3: list<f64>                                           │  │   │
│  │  └───────────────────────────────────────────────────────────────┘  │   │
│  │                                                                       │   │
│  │  ┌───────────────────────────────────────────────────────────────┐  │   │
│  │  │ Metadata:                                                     │  │   │
│  │  │   • version: u64                                              │  │   │
│  │  │   • config: {...}                                             │  │   │
│  │  │   • schema_metadata: {...}                                    │  │   │
│  │  │   • reader_flags: {...}                                       │  │   │
│  │  │   • writer_flags: {...}                                       │  │   │
│  │  └───────────────────────────────────────────────────────────────┘  │   │
│  │                                                                       │   │
│  │  ┌───────────────────────────────────────────────────────────────┐  │   │
│  │  │ Fragments:                                                     │  │   │
│  │  │   • fragment 1 ────────────────────┐                          │  │   │
│  │  │   • fragment 2                     │                          │  │   │
│  │  │   • fragment 3                     │                          │  │   │
│  │  └─────────────────────────────────────┼──────────────────────────┘  │   │
│  │                                        │                              │   │
│  │  ──────────────────────────────────────┼────────────────────────────  │   │
│  │                                        │                              │   │
│  │  ┌───────────────────────────────────────────────────────────────┐  │   │
│  │  │ Index Section:                                                 │  │   │
│  │  │   • BTree Index (col1) ────────────┐                          │  │   │
│  │  │   • Full-text Index (col2) ────────┼───┐                       │  │   │
│  │  │   • Vector Index (col3) ────────────┼───┼───┐                   │  │   │
│  │  └─────────────────────────────────────┼───┼───┼───────────────────┘  │   │
│  └────────────────────────────────────────┼───┼───┼───────────────────────┘   │
│                                           │   │   │                            │
│                                           │   │   │                            │
│  ┌────────────────────────────────────────┘   │   │                            │
│  │         TRANSACTION FILE                    │   │                            │
│  │  (Versioning & Atomic Updates)              │   │                            │
│  └─────────────────────────────────────────────┘   │                            │
│                                                     │                            │
│                                                     ▼                            │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │                           FRAGMENT 1                                     │  │
│  │  ┌──────────────────────────────────────────────────────────────────┐  │  │
│  │  │ Fragment Metadata:                                                │  │  │
│  │  │   • fragment_id: 1                                                │  │  │
│  │  │   • physical_rows: usize                                         │  │  │
│  │  │   • row_id_meta: RowIdMeta                                       │  │  │
│  │  └──────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                          │  │
│  │  ┌──────────────────────────────────────────────────────────────────┐  │  │
│  │  │ Row ID & Lineage Metadata:                                        │  │  │
│  │  │   • row_id_sequence: [0, 1, 2, ...]                               │  │  │
│  │  │   • created_at_version: u64                                       │  │  │
│  │  │   • last_updated_at_version: u64                                  │  │  │
│  │  └──────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                          │  │
│  │  ┌──────────────────────────────────┐  ┌─────────────────────────────┐ │  │
│  │  │   DATA FILE (col1, col2)         │  │   DATA FILE (col3)          │ │  │
│  │  │   data/{uuid1}.lance             │  │   data/{uuid2}.lance        │ │  │
│  │  │                                  │  │                             │ │  │
│  │  │   ┌──────────────────────────┐  │  │   ┌──────────────────────┐ │ │  │
│  │  │   │ Data Pages:              │  │  │   │ Data Pages:          │ │ │  │
│  │  │   │   • col1 pages           │  │  │   │   • col3 pages        │ │ │  │
│  │  │   │   • col2 pages           │  │  │   │                      │ │ │  │
│  │  │   └──────────────────────────┘  │  │   └──────────────────────┘ │ │  │
│  │  │   ┌──────────────────────────┐  │  │   ┌──────────────────────┐ │ │  │
│  │  │   │ Column Metadata:        │  │  │   │ Column Metadata:      │ │ │  │
│  │  │   │   • field_ids: [0, 1]    │  │  │   │   • field_id: [2]    │ │ │  │
│  │  │   │   • column_indices      │  │  │   │   • column_indices    │ │ │  │
│  │  │   │   • page offsets/sizes   │  │  │   │   • page offsets/sizes │ │ │  │
│  │  │   │   • encoding info       │  │  │   │   • encoding info     │ │ │  │
│  │  │   └──────────────────────────┘  │  │   └──────────────────────┘ │ │  │
│  │  │   ┌──────────────────────────┐  │  │   ┌──────────────────────┐ │ │  │
│  │  │   │ Global Buffers:         │  │  │   │ Global Buffers:       │ │ │  │
│  │  │   │   • Schema               │  │  │   │   • Schema            │ │ │  │
│  │  │   │   • Shared dictionaries  │  │  │   │   • Shared dictionaries│ │ │  │
│  │  │   └──────────────────────────┘  │  │   └──────────────────────┘ │ │  │
│  │  │   ┌──────────────────────────┐  │  │   ┌──────────────────────┐ │ │  │
│  │  │   │ Footer:                 │  │  │   │ Footer:              │ │ │  │
│  │  │   │   • Metadata offsets    │  │  │   │   • Metadata offsets  │ │ │  │
│  │  │   │   • Format version      │  │  │   │   • Format version    │ │ │  │
│  │  │   │   • Magic (LANC)        │  │  │   │   • Magic (LANC)      │ │ │  │
│  │  │   └──────────────────────────┘  │  │   └──────────────────────┘ │ │  │
│  │  └──────────────────────────────────┘  └─────────────────────────────┘ │  │
│  │                                                                          │  │
│  │  ┌──────────────────────────────────────────────────────────────────┐  │  │
│  │  │   DELETION FILE                                                  │  │  │
│  │  │   _deletions/{fragment-id}.{id}.arrow_bin                        │  │  │
│  │  │                                                                  │  │  │
│  │  │   • Deleted row offsets (local within fragment)                  │  │  │
│  │  │   • Format: Array or Bitmap                                     │  │  │
│  │  │   • num_deleted_rows: usize                                     │  │  │
│  │  └──────────────────────────────────────────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                                                                                  │
│                                                                                  │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │                              INDICES                                      │  │
│  │                                                                          │  │
│  │  ┌──────────────────────┐  ┌──────────────────────┐  ┌───────────────┐ │  │
│  │  │  BTREE INDEX        │  │  FULL-TEXT INDEX     │  │ VECTOR INDEX  │ │  │
│  │  │  (col1)              │  │  (col2)              │  │ (col3)        │ │  │
│  │  │                      │  │                      │  │               │ │  │
│  │  │  _indices/{uuid}/    │  │  _indices/{uuid}/    │  │ _indices/     │ │  │
│  │  │  index.idx           │  │  index.idx            │  │ {uuid}/       │ │  │
│  │  │                      │  │                      │  │ index.idx     │ │  │
│  │  │  • BTree structure   │  │  • Inverted index    │  │ • Vector      │ │  │
│  │  │  • Fragment bitmap   │  │  • Fragment bitmap   │  │   embeddings  │ │  │
│  │  │  • Key → RowAddr     │  │  • Token → RowAddr   │  │ • Fragment    │ │  │
│  │  │                      │  │                      │  │   bitmap      │ │  │
│  │  │                      │  │                      │  │ • Similarity  │ │  │
│  │  │                      │  │                      │  │   search      │ │  │
│  │  └──────────────────────┘  └──────────────────────┘  └───────────────┘ │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

## Key Relationships

### 1. Manifest → Fragments
- **One Manifest** contains references to **multiple Fragments** (fragment 1, 2, 3, ...)
- Each fragment entry in manifest contains:
  - Fragment ID
  - List of DataFile references
  - Physical row count
  - Row ID sequence
  - Deletion file reference

### 2. Fragment → Data Files
- **One Fragment** can contain **multiple Data Files** (e.g., Fragment 1 has Data File for col1/col2 and Data File for col3)
- Each Data File belongs to **exactly one Fragment**
- All Data Files in a fragment must have the same number of rows

### 3. Data File Structure
Each `.lance` data file contains:
- **Data Pages**: Variable number of pages per column (encoded/compressed)
- **Column Metadata**: Field IDs, column indices, page offsets, encoding info
- **Global Buffers**: Schema, shared dictionaries
- **Footer**: Metadata offsets, format version, magic number

### 4. Fragment → Deletion File
- **One Fragment** can have **one Deletion File** (optional)
- Tracks logically deleted rows without rewriting data files

### 5. Manifest → Indices
- **Manifest** contains metadata about **multiple Indices**
- Each index can be:
  - **BTree Index**: For scalar range/exact queries
  - **Full-text Index**: For text search
  - **Vector Index**: For similarity search on embeddings

### 6. Transaction File → Manifest
- **Transaction File** manages versioning and atomic updates
- Each write creates a new manifest version
- Ensures ACID properties

## File System Layout

```
dataset/
├── _versions/
│   ├── 1.manifest          ← Manifest (version 1)
│   ├── 2.manifest          ← Manifest (version 2)
│   └── latest.manifest     ← Pointer to latest version
│
├── data/
│   ├── {uuid1}.lance       ← Data File (col1, col2) for Fragment 1
│   ├── {uuid2}.lance       ← Data File (col3) for Fragment 1
│   ├── {uuid3}.lance       ← Data File for Fragment 2
│   └── {uuid4}.lance       ← Data File for Fragment 3
│
├── _deletions/
│   └── {fragment-id}.{id}.arrow_bin  ← Deletion File for Fragment 1
│
└── _indices/
    ├── {uuid-btree}/index.idx        ← BTree Index (col1)
    ├── {uuid-fulltext}/index.idx     ← Full-text Index (col2)
    └── {uuid-vector}/index.idx       ← Vector Index (col3)
```

## Important Concepts

1. **Fragments are Logical**: Fragments exist only in the manifest, not as physical files
2. **One Fragment → Multiple Files**: When columns are added separately, a fragment can reference multiple `.lance` files
3. **One File → One Fragment**: Each `.lance` file belongs to exactly one fragment
4. **Variable Pages**: Each column can have different numbers of pages based on data size
5. **Versioning**: Each write creates a new manifest version via Transaction File
6. **Indices are Separate**: Indices are stored separately but referenced in manifest

