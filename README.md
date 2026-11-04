# uniformat-core

The shared foundation of the UniFormat family of file-format readers and writers.

- **Data model:** Apache Arrow (`RecordBatch`, `Schema`). Format-specific metadata travels in the schema under the
  prefix `<format>.`.
- **Interfaces:** `Format` (info / read / write), `Registry` (lookup by name or extension), `convert()` (format → Arrow →
  format).
- **Building blocks:** `ByteSource` (range reads: file, memory, counting wrapper) and `AtomicFile` (write to a temporary
  file, rename on commit, nothing left behind on error).
- **No format inside.** Format crates (`uniformat-sas`, …) depend on this crate, never the reverse.

## Layout

| Path | Content |
|---|---|
| `crates/uniformat-core` | Rust crate `uniformat-core` |
| `python/` | Python distribution `uniformat-core`, import `uniformat_io.core` (PEP 420 namespace, pure Python) |

## Rust

```rust
use uniformat_core::{Registry, ReadRequest, WriteRequest, convert};

let mut registry = Registry::new();
// each format crate provides `register(&mut registry)`
// uniformat_sas_arrow::register(&mut registry);
convert(&registry, "in.sas7bdat".as_ref(), "out.sas7bdat".as_ref(), None, None,
        &ReadRequest::default(), &WriteRequest::default())?;
```

## Python

```python
import uniformat_io.core as uf
table = uf.read("data.sas7bdat")                 # needs uniformat-sas installed
uf.convert("data.sas7bdat", "copy.sas7bdat", write_options={"compression": "char"})
```

Format distributions register through the `uniformat_io.formats` entry-point group.

## Development

```bash
cargo test            # Rust
cd python && pip install -e . pytest && pytest   # Python
```

Sibling repositories are expected side by side (`../uniformat-sas`, …) because Rust path dependencies point to them.

Status: 0.1, not published. License: `MIT OR Apache-2.0` (provisional).

## Security and reproducible checks

Atomic writes create private temporary files in the destination directory. On Unix, new files use mode 0600 and replacing a regular file preserves its ordinary permission bits. Symlink destinations are refused. Ownership, special mode bits and extended ACLs are not copied; use a trusted destination directory with an appropriate access policy.

The workspace lockfile is tracked so CI audits the same Rust resolution as the tests. Run `cargo test --locked` and `cargo deny --locked --all-features check advisories`. The tests include permission preservation and cleanup on failure. Python CI installs `ci/requirements-python.txt` with hashes and runs pip-audit. Regenerate that lock from its `.in` file with the command recorded in its header, then audit and test the new resolution.
