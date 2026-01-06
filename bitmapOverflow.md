# info
> - command to reproduce
>	- `LANCE_LOG=info RUST_BACKTRACE=FULL  cargo test scalar::bitmap::tests::test_big_bitmap_index -- --exact --nocapture`
> - broken python test
```
maturin develop && ../.venv/bin/python -m pytest python/tests/test_scalar_index.py -k test_create_index_empty_dataset
```

running 1 test
test scalar::bitmap::tests::test_test_big_bitmap_index has been running for over 60 seconds
write_bitmap_index: DEBUG: state size 2_500_000

thread 'scalar::bitmap::tests::test_test_big_bitmap_index' panicked at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/arrow-array-54.2.1/src/builder/generic_bytes_builder.rs:86:57:
byte array offset overflow
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test scalar::bitmap::tests::test_test_big_bitmap_index ... FAILED

failures:

failures:
    scalar::bitmap::tests::test_test_big_bitmap_index

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 61 filtered out; finished in 621.37s

error: test failed, to rerun pass `-p lance-index --lib`


## Problem
> - when the input has more than 2^31-1 values, it does not fit within a single Arrow BinaryArray. You need to do chunkcation on both read and write path.


## Possible fix
> - chunk the keys and the bitmaps.
```Rust
// ...existing code...

const MAX_ARROW_ARRAY_LEN: usize = std::i32::MAX as usize - 1024 * 1024; // leave headroom

fn chunked_keys_and_bitmaps(
    state: HashMap<ScalarValue, RowIdTreeMap>,
    value_type: &DataType,
) -> Vec<(Arc<dyn Array>, Arc<dyn Array>)> {
    let mut batches = Vec::new();
    let mut cur_keys = Vec::new();
    let mut cur_bitmaps = Vec::new();
    let mut cur_bytes = 0;

    for (key, bitmap) in state.into_iter() {
        let mut bytes = Vec::new();
        bitmap.serialize_into(&mut bytes).unwrap();
        let bitmap_len = bytes.len();

        // If adding this bitmap would overflow, flush current batch
        if cur_keys.len() >= MAX_ARROW_ARRAY_LEN || cur_bytes + bitmap_len > MAX_ARROW_ARRAY_LEN {
            let keys_array = if cur_keys.is_empty() {
                new_empty_array(value_type)
            } else {
                ScalarValue::iter_to_array(cur_keys.clone().into_iter()).unwrap()
            };
            let mut binary_builder = BinaryBuilder::new();
            for b in &cur_bitmaps {
                binary_builder.append_value(b);
            }
            let bitmaps_array = Arc::new(binary_builder.finish()) as Arc<dyn Array>;
            batches.push((keys_array, bitmaps_array));
            cur_keys.clear();
            cur_bitmaps.clear();
            cur_bytes = 0;
        }

        cur_keys.push(key);
        cur_bitmaps.push(bytes);
        cur_bytes += bitmap_len;
    }

    // Flush any remaining
    if !cur_keys.is_empty() {
        let keys_array = ScalarValue::iter_to_array(cur_keys.into_iter()).unwrap();
        let mut binary_builder = BinaryBuilder::new();
        for b in &cur_bitmaps {
            binary_builder.append_value(b);
        }
        let bitmaps_array = Arc::new(binary_builder.finish()) as Arc<dyn Array>;
        batches.push((keys_array, bitmaps_array));
    }

    batches
}

async fn write_bitmap_index(
    state: HashMap<ScalarValue, RowIdTreeMap>,
    index_store: &dyn IndexStore,
    value_type: &DataType,
) -> Result<()> {
    println!("write_bitmap_index: DEBUG: state size {}", state.len());

    let batches = chunked_keys_and_bitmaps(state, value_type);

    for (i, (keys_array, binary_bitmap_array)) in batches.into_iter().enumerate() {
        let record_batch = get_batch_from_arrays(keys_array, binary_bitmap_array)?;
        println!(
            "DEBUG: Writing bitmap index batch {} with {} rows",
            i,
            record_batch.num_rows()
        );

        let mut bitmap_index_file = index_store
            .new_index_file(BITMAP_LOOKUP_NAME, record_batch.schema())
            .await?;
        bitmap_index_file.write_record_batch(record_batch).await?;
        bitmap_index_file.finish().await?;
    }
    Ok(())
}
// ...existing code...
```



> - Use LargeBinaryMapBuilder
```Rust
use arrow_array::LargeBinaryBuilder;

fn get_bitmaps_from_iter<I>(iter: I) -> Arc<dyn Array>
where
    I: Iterator<Item = RowIdTreeMap>,
{
    let mut builder = LargeBinaryBuilder::new();
    for bitmap in iter {
        let mut bytes = Vec::new();
        bitmap.serialize_into(&mut bytes).unwrap();
        builder.append_value(&bytes).unwrap();
    }
    Arc::new(builder.finish())
}
```
> - concatent
```Rust
use arrow::error::Result as ArrowResult;
use arrow::compute::concat;
use arrow::array::{BinaryArray, BinaryBuilder};

fn get_bitmaps_from_iter<I>(iter: I) -> ArrowResult<Arc<dyn Array>>
where
    I: Iterator<Item = RowIdTreeMap>,
{
    // Maximum total bytes that BinaryBuilder can handle.
    const MAX_BYTES: usize = std::i32::MAX as usize;

    // Store finished BinaryArrays (chunks)
    let mut chunks = Vec::new();
    // Create a new BinaryBuilder for the current chunk.
    let mut builder = BinaryBuilder::new();
    // Track the cumulative number of bytes in the current builder.
    let mut current_total: usize = 0;

    for bitmap in iter {
        let mut bytes = Vec::new();
        bitmap.serialize_into(&mut bytes).unwrap();
        // If adding this value would exceed the limit, finish the current chunk.
        if current_total + bytes.len() > MAX_BYTES {
            chunks.push(builder.finish());
            builder = BinaryBuilder::new();
            current_total = 0;
        }

        builder.append_value(&bytes).unwrap();
        current_total += bytes.len();
    }

    // Push the final chunk if it contains any elements.
    if builder.len() > 0 {
        chunks.push(builder.finish());
    }

    // If we only have one chunk, return it directly.
    if chunks.len() == 1 {
        Ok(Arc::new(chunks.pop().unwrap()))
    } else {
        // Otherwise, concatenate all chunks into a single array.
        let concatenated = concat(&chunks)?;
        Ok(Arc::new(concatenated))
    }
}
```

> - new error
```
write_bitmap_index: DEBUG: state size 2500000                                                                    
write_bitmap_index: DEBUG: values_iter size 2500000                                                              
TRIGGER OVERFLOW: cur_total 1073740824 + bytes.len() 2028                                                        
TRIGGER OVERFLOW: cur_total 1073740824 + bytes.len() 2028                                                        
TRIGGER OVERFLOW: cur_total 1073740824 + bytes.len() 2028                                                        
TRIGGER OVERFLOW: cur_total 1073740824 + bytes.len() 2028                                                        
DEBUG: finish building                                                                                           
                                                                                                                 
thread 'scalar::bitmap::tests::test_test_big_bitmap_index' panicked at /Users/haochengliu/.cargo/registry/src/index.crates.io
-1949cf8c6b5b557f/arrow-data-54.3.1/src/transform/utils.rs:42:56:                                                
offset overflow                                                                                                  
stack backtrace:                                                                                                 
   0: rust_begin_unwind                                                                                          
             at /rustc/05f9846f893b09a1be1fc8560e33fc3c815cfecb/library/std/src/panicking.rs:695:5               
   1: core::panicking::panic_fmt                                                                                 
             at /rustc/05f9846f893b09a1be1fc8560e33fc3c815cfecb/library/core/src/panicking.rs:75:14              
   2: core::panicking::panic_display                                                                             
             at /rustc/05f9846f893b09a1be1fc8560e33fc3c815cfecb/library/core/src/panicking.rs:261:5              
   3: core::option::expect_failed                                                                                
             at /rustc/05f9846f893b09a1be1fc8560e33fc3c815cfecb/library/core/src/option.rs:2024:5                
   4: core::option::Option<T>::expect                                                                            
             at /Users/haochengliu/.rustup/toolchains/1.86.0-aarch64-apple-darwin/lib/rustlib/src/rust/library/co
re/src/option.rs:933:21                                                                                          
   5: arrow_data::transform::utils::extend_offsets::{{closure}}                                                  
             at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/arrow-data-54.3.1/src/tra
nsform/utils.rs:42:23                                                                                            
   6: core::iter::traits::iterator::Iterator::for_each::call::{{closure}}                                        
             at /Users/haochengliu/.rustup/toolchains/1.86.0-aarch64-apple-darwin/lib/rustlib/src/rust/library/co
re/src/iter/traits/iterator.rs:797:29                                                                            
   7: core::iter::traits::iterator::Iterator::fold                                                               
             at /Users/haochengliu/.rustup/toolchains/1.86.0-aarch64-apple-darwin/lib/rustlib/src/rust/library/co
re/src/iter/traits/iterator.rs:2546:21                                                                           
   8: core::iter::traits::iterator::Iterator::for_each                                                           
             at /Users/haochengliu/.rustup/toolchains/1.86.0-aarch64-apple-darwin/lib/rustlib/src/rust/library/co
re/src/iter/traits/iterator.rs:800:9                                                                             
   9: arrow_data::transform::utils::extend_offsets                                                               
             at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/arrow-data-54.3.1/src/tra
nsform/utils.rs:36:5                                                                                             
  10: arrow_data::transform::variable_size::build_extend::{{closure}}                                            
             at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/arrow-data-54.3.1/src/tra
nsform/variable_size.rs:55:13                                                                                    
  11: <alloc::boxed::Box<F,A> as core::ops::function::Fn<Args>>::call                                            
             at /Users/haochengliu/.rustup/toolchains/1.86.0-aarch64-apple-darwin/lib/rustlib/src/rust/library/al
loc/src/boxed.rs:1990:9                                                                                          
  12: arrow_data::transform::MutableArrayData::extend                                                            
             at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/arrow-data-54.3.1/src/tra
nsform/mod.rs:722:9                                                                                              
  13: arrow_select::concat::concat_fallback                                                                      
             at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/arrow-select-54.2.1/src/c
oncat.rs:256:9                                                                                                   
  14: arrow_select::concat::concat                                                                               
             at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/arrow-select-54.2.1/src/c
oncat.rs:242:13
  15: lance_index::scalar::bitmap::get_bitmaps_from_iter
             at ./src/scalar/bitmap.rs:369:28
  16: lance_index::scalar::bitmap::write_bitmap_index::{{closure}}
             at ./src/scalar/bitmap.rs:393:31
  17: lance_index::scalar::bitmap::tests::test_test_big_bitmap_index::{{closure}}
             at ./src/scalar/bitmap.rs:506:94
  18: <core::pin::Pin<P> as core::future::future::Future>::poll
             at /Users/haochengliu/.rustup/toolchains/1.86.0-aarch64-apple-darwin/lib/rustlib/src/rust/library/co
re/src/future/future.rs:124:9
  19: <core::pin::Pin<P> as core::future::future::Future>::poll
             at /Users/haochengliu/.rustup/toolchains/1.86.0-aarch64-apple-darwin/lib/rustlib/src/rust/library/co
re/src/future/future.rs:124:9
  20: tokio::runtime::scheduler::current_thread::CoreGuard::block_on::{{closure}}::{{closure}}::{{closure}}
             at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.44.2/src/runtime/
scheduler/current_thread/mod.rs:733:54
  21: tokio::task::coop::with_budget
             at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.44.2/src/task/coo
p/mod.rs:167:5
  22: tokio::task::coop::budget
             at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.44.2/src/task/coo
p/mod.rs:133:5
  23: tokio::runtime::scheduler::current_thread::CoreGuard::block_on::{{closure}}::{{closure}}
             at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.44.2/src/runtime/
scheduler/current_thread/mod.rs:733:25
  24: tokio::runtime::scheduler::current_thread::Context::enter
             at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.44.2/src/runtime/
scheduler/current_thread/mod.rs:432:19
  25: tokio::runtime::scheduler::current_thread::CoreGuard::block_on::{{closure}}
             at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.44.2/src/runtime/
scheduler/current_thread/mod.rs:732:36
  26: tokio::runtime::scheduler::current_thread::CoreGuard::enter::{{closure}}
             at /Users/haochengliu/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.44.2/src/runtime/
scheduler/current_thread/mod.rs:820:68
  27: tokio::runtime::context::scoped::Scoped<T>::set
                                             
 ```
 > - type mismatch in trest_create_index_empty_datast
 ```
 =================================================================== test session starts ===================================================================
platform darwin -- Python 3.12.10, pytest-8.3.5, pluggy-1.5.0 -- /Users/haochengliu/Documents/projects/lance/.venv/bin/python
cachedir: .pytest_cache
rootdir: /Users/haochengliu/Documents/projects/lance/python
configfile: pyproject.toml
collected 44 items / 43 deselected / 1 selected                                                                                                           

python/tests/test_scalar_index.py::test_create_index_empty_dataset write_bitmap_index: DEBUG: state size 0
write_bitmap_index: DEBUG: state size 0
DEBUG: Dataset contents:
pyarrow.Table
btree: int32
bitmap: int32
label_list: list<item: string>
  child 0, item: string
inverted: string
ngram: string
----
btree: [[1]]
bitmap: [[1]]
label_list: [[["foo","bar"]]]
inverted: [["blah"]]
ngram: [["apple"]]
Loading bitmap index with 0 total rows
Warning: Empty bitmap index, using default value type
Successfully loaded bitmap index with 0 keys
Loading bitmap index with 0 total rows
Warning: Empty bitmap index, using default value type
Successfully loaded bitmap index with 0 keys
1078: about to call optimize_indices
write_bitmap_index: DEBUG: state size 3
Warning: Error creating keys array: Execution error: Inconsistent types in ScalarValue::iter_to_array. Expected Utf8, got Int32(NULL)
DEBUG: Writing bitmap index batch 0 with 3 rows
write_bitmap_index: DEBUG: state size 2
DEBUG: Writing bitmap index batch 0 with 2 rows
Loading bitmap index with 2 total rows
Loading rows 0 to 2
Successfully loaded bitmap index with 1 keys
Loading bitmap index with 3 total rows
Loading rows 0 to 3
Successfully loaded bitmap index with 2 keys
PASSED
 ```

> - local test file
```
        // let test_store = LanceIndexStore::new(
        //     Arc::new(ObjectStore::local()),
        //     Path::from_filesystem_path("/Users/haochengliu/Documents/projects/lance/big_data")
        //         .unwrap(),
        //     FileMetadataCache::no_cache(),
        // );
``` 