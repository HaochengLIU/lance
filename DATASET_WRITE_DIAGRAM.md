# Lance Dataset Write Process - High Level Diagram

## Overview
This diagram illustrates how dataset writes work in Lance, including what files are generated and their contents.

## Write Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Dataset Write Operation                          │
└─────────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 1. Input: RecordBatch Stream                                            │
│    - Arrow RecordBatches with schema                                    │
│    - Data is chunked/broken into manageable sizes                       │
└─────────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 2. Fragment Creation                                                     │
│    - Each fragment gets a unique ID (u64)                               │
│    - Fragment is a logical grouping of rows                             │
│    - Multiple fragments can be created in parallel                      │
└─────────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 3. Data File Writing (per fragment)                                    │
│    ┌──────────────────────────────────────────────────────────────┐    │
│    │ File: data/{UUID}.lance                                      │    │
│    │                                                               │    │
│    │ Structure:                                                     │    │
│    │ ┌─────────────────────────────────────────────────────────┐   │    │
│    │ │ Data Pages (variable number)                           │   │    │
│    │ │ - Each column has multiple pages                       │   │    │
│    │ │ - Pages contain encoded/compressed data buffers        │   │    │
│    │ │ - Page metadata: offsets, sizes, encoding info        │   │    │
│    │ └─────────────────────────────────────────────────────────┘   │    │
│    │ ┌─────────────────────────────────────────────────────────┐   │    │
│    │ │ Column Metadata (per column)                            │   │    │
│    │ │ - Field IDs                                            │   │    │
│    │ │ - Column indices                                       │   │    │
│    │ │ - Page list with buffer offsets/sizes                  │   │    │
│    │ │ - Encoding descriptors                                 │   │    │
│    │ └─────────────────────────────────────────────────────────┘   │    │
│    │ ┌─────────────────────────────────────────────────────────┐   │    │
│    │ │ Global Buffers                                          │   │    │
│    │ │ - Schema (Arrow schema)                                 │   │    │
│    │ │ - Shared dictionaries                                   │   │    │
│    │ └─────────────────────────────────────────────────────────┘   │    │
│    │ ┌─────────────────────────────────────────────────────────┐   │    │
│    │ │ Footer                                                  │   │    │
│    │ │ - Column metadata offsets                               │   │    │
│    │ │ - Global buffer offsets                                 │   │    │
│    │ │ - File format version                                   │   │    │
│    │ │ - Magic number (LANC)                                   │   │    │
│    │ └─────────────────────────────────────────────────────────┘   │    │
│    └──────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 4. Fragment Metadata Creation                                           │
│    - Fragment ID                                                        │
│    - List of DataFile references (paths, field IDs, column indices)    │
│    - Physical row count                                                 │
│    - Row ID sequence (optional)                                         │
│    - Deletion file reference (optional)                                 │
└─────────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 5. Transaction Building                                                  │
│    - Collects all fragments                                             │
│    - Tracks affected rows                                                │
│    - Prepares manifest updates                                          │
└─────────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ 6. Commit Process                                                        │
│    ┌──────────────────────────────────────────────────────────────┐    │
│    │ Write Manifest File                                          │    │
│    │ File: _versions/{version}.manifest                           │    │
│    │                                                               │    │
│    │ Contents:                                                     │    │
│    │ - Schema                                                      │    │
│    │ - Fragment list (with DataFile references)                   │    │
│    │ - Version number                                             │    │
│    │ - Timestamp                                                  │    │
│    │ - Indices metadata (references to index files)              │    │
│    │ - Statistics (optional)                                      │    │
│    │ - Feature flags                                              │    │
│    └──────────────────────────────────────────────────────────────┘    │
│    ┌──────────────────────────────────────────────────────────────┐    │
│    │ Update Version Pointer                                       │    │
│    │ File: _versions/latest.manifest (symlink or pointer)        │    │
│    └──────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────┘
```

## Directory Structure After Write

```
dataset/
├── data/                                    # Data files directory
│   ├── {uuid1}.lance                       # Fragment 0 data file
│   │   ├── Data pages (column data)
│   │   ├── Column metadata
│   │   ├── Global buffers (schema, dictionaries)
│   │   └── Footer (metadata offsets, version, magic)
│   │
│   ├── {uuid2}.lance                       # Fragment 1 data file
│   │   └── (same structure)
│   │
│   └── {uuidN}.lance                       # Fragment N data file
│       └── (same structure)
│
├── _versions/                              # Version metadata
│   ├── 1.manifest                          # Version 1 manifest
│   │   ├── Schema
│   │   ├── Fragments list
│   │   │   └── Each fragment:
│   │   │       ├── Fragment ID
│   │   │       ├── DataFile references
│   │   │       ├── Physical row count
│   │   │       └── Row ID sequence (optional)
│   │   ├── Version number
│   │   ├── Timestamp
│   │   └── Indices metadata
│   │
│   ├── 2.manifest                          # Version 2 manifest
│   └── latest.manifest                     # Pointer to latest version
│
├── _indices/                               # Secondary indices (optional)
│   └── {index-uuid}/
│       └── index.idx
│
└── _deletions/                             # Deletion files (optional)
    └── {fragment-id}.{deletion-id}.arrow_bin
```

## Key File Types and Contents

### 1. Data File (`data/{UUID}.lance`)
**Purpose**: Stores actual columnar data for a fragment

**Contents**:
- **Data Pages**: 
  - Multiple pages per column
  - Encoded/compressed data buffers
  - Variable number of pages based on data size
- **Column Metadata**:
  - Field IDs and column indices
  - Page list with buffer offsets and sizes
  - Encoding descriptors (compression, encoding type)
- **Global Buffers**:
  - Arrow schema
  - Shared dictionaries (for dictionary encoding)
- **Footer**:
  - Column metadata offsets
  - Global buffer offsets
  - File format version (major.minor)
  - Magic number ("LANC")

### 2. Manifest File (`_versions/{version}.manifest`)
**Purpose**: Tracks dataset structure and version history

**Contents**:
- **Schema**: Full Arrow schema definition
- **Fragments List**: 
  - Fragment ID
  - List of DataFile references (paths, field IDs, column indices)
  - Physical row count
  - Row ID sequence (optional, for stable row IDs)
  - Deletion file reference (optional)
- **Version Metadata**:
  - Version number
  - Timestamp
  - Writer information
- **Indices Metadata**: References to index files in `_indices/`
- **Statistics**: Optional column-level statistics
- **Feature Flags**: Format capabilities

### 3. Deletion File (`_deletions/{fragment-id}.{id}.arrow_bin`)
**Purpose**: Tracks deleted rows within a fragment

**Contents**:
- Local row offsets of deleted rows
- Can be Array or Bitmap format

### 4. Index File (`_indices/{uuid}/index.idx`)
**Purpose**: Secondary indices for fast lookups

**Contents**:
- Vector indices (for similarity search)
- Scalar indices (BTree, inverted, etc.)
- Fragment bitmap (which fragments are indexed)

## Write Process Summary

1. **Input Processing**: RecordBatches are received and chunked
2. **Fragment Creation**: Each fragment gets a unique ID and groups rows
3. **Data File Writing**: For each fragment:
   - Create a `.lance` file with UUID name
   - Write data pages (encoded/compressed)
   - Write column metadata
   - Write global buffers (schema, dictionaries)
   - Write footer with offsets
4. **Fragment Metadata**: Create Fragment struct with:
   - Fragment ID
   - DataFile references
   - Row counts
   - Row ID sequences
5. **Transaction**: Collect fragments and prepare manifest
6. **Commit**: Write manifest file and update version pointer

## Important Notes

- **Fragments are logical, not physical**: They exist only in the manifest, not as separate files
- **One fragment → Multiple data files**: When columns are added separately, a fragment can reference multiple `.lance` files
- **One data file → One fragment**: Each `.lance` file belongs to exactly one fragment (tracked in manifest)
- **Pages are variable**: Each column can have different numbers of pages based on data size
- **Versioning**: Each write creates a new manifest version, enabling time travel
- **ACID guarantees**: Commit process ensures atomic updates via manifest versioning

