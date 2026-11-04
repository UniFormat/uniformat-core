//! Registry and conversion through the common model, with two in-memory test formats.

use std::path::Path;
use std::sync::{Arc, Mutex};

use arrow_array::{ArrayRef, Float64Array, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use uniformat_core::{
    convert, BatchStream, Error, Format, ReadRequest, Registry, TableInfo, WriteRequest,
    WriteSummary,
};

/// A format backed by a shared slot, standing for "a file".
struct Memory {
    name: &'static str,
    ext: &'static [&'static str],
    slot: Arc<Mutex<Vec<RecordBatch>>>,
    writable: bool,
}

impl Format for Memory {
    fn name(&self) -> &'static str {
        self.name
    }
    fn extensions(&self) -> &'static [&'static str] {
        self.ext
    }
    fn can_write(&self) -> bool {
        self.writable
    }
    fn info(&self, _: &Path, _: &ReadRequest) -> Result<TableInfo, Error> {
        let b = self.slot.lock().unwrap();
        let schema = b
            .first()
            .map(RecordBatch::schema)
            .ok_or_else(|| Error::format(self.name, "empty"))?;
        Ok(TableInfo {
            schema,
            row_count: Some(b.iter().map(|x| x.num_rows() as u64).sum()),
        })
    }
    fn read(&self, _: &Path, req: &ReadRequest) -> Result<BatchStream, Error> {
        let batches = self.slot.lock().unwrap().clone();
        let schema = batches
            .first()
            .map(RecordBatch::schema)
            .ok_or_else(|| Error::format(self.name, "empty"))?;
        let mut out = Vec::new();
        let mut left = req.limit.unwrap_or(u64::MAX);
        for b in batches {
            let n = (b.num_rows() as u64).min(left) as usize;
            out.push(Ok(b.slice(0, n)));
            left -= n as u64;
        }
        Ok(Box::new(RecordBatchIterator::new(out, schema)))
    }
    fn write(
        &self,
        _: &Path,
        _: SchemaRef,
        batches: BatchStream,
        _: &WriteRequest,
    ) -> Result<WriteSummary, Error> {
        if !self.writable {
            return Err(Error::Unsupported("read-only".into()));
        }
        let mut rows = 0;
        let mut slot = self.slot.lock().unwrap();
        slot.clear();
        for b in batches {
            let b = b?;
            rows += b.num_rows() as u64;
            slot.push(b);
        }
        Ok(WriteSummary {
            rows,
            warnings: vec![],
        })
    }
}

fn table() -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("x", DataType::Float64, true),
        Field::new("s", DataType::Utf8, true),
    ]));
    let cols: Vec<ArrayRef> = vec![
        Arc::new(Float64Array::from(vec![Some(1.5), None, Some(3.0)])),
        Arc::new(StringArray::from(vec!["a", "b", "c"])),
    ];
    RecordBatch::try_new(schema, cols).unwrap()
}

#[test]
fn lookup_by_name_and_extension() {
    let slot = Arc::new(Mutex::new(vec![]));
    let mut r = Registry::new();
    r.register(Arc::new(Memory {
        name: "alpha",
        ext: &["a", "alp"],
        slot: slot.clone(),
        writable: true,
    }));
    r.register(Arc::new(Memory {
        name: "beta",
        ext: &["b"],
        slot,
        writable: false,
    }));
    assert_eq!(r.by_name("ALPHA").unwrap().name(), "alpha");
    assert_eq!(
        r.by_path(Path::new("dir/file.ALP")).unwrap().name(),
        "alpha"
    );
    assert!(matches!(
        r.by_path(Path::new("file.zzz")),
        Err(Error::UnknownFormat(_))
    ));
    assert!(matches!(
        r.by_path(Path::new("noext")),
        Err(Error::UnknownFormat(_))
    ));
    assert!(matches!(r.by_name("gamma"), Err(Error::UnknownFormat(_))));
    // re-registering replaces
    r.register(Arc::new(Memory {
        name: "alpha",
        ext: &["a2"],
        slot: Arc::new(Mutex::new(vec![])),
        writable: true,
    }));
    assert_eq!(r.formats().len(), 2);
    assert!(r.by_path(Path::new("f.a")).is_err());
}

#[test]
fn conversion_goes_through_the_common_model() {
    let src_slot = Arc::new(Mutex::new(vec![table()]));
    let dst_slot = Arc::new(Mutex::new(vec![]));
    let mut r = Registry::new();
    r.register(Arc::new(Memory {
        name: "alpha",
        ext: &["a"],
        slot: src_slot,
        writable: true,
    }));
    r.register(Arc::new(Memory {
        name: "beta",
        ext: &["b"],
        slot: dst_slot.clone(),
        writable: true,
    }));
    let s = convert(
        &r,
        Path::new("in.a"),
        Path::new("out.b"),
        None,
        None,
        &ReadRequest::default(),
        &WriteRequest::default(),
    )
    .unwrap();
    assert_eq!(s.rows, 3);
    assert_eq!(dst_slot.lock().unwrap()[0], table());
    // LIMIT is honoured by the reader
    let req = ReadRequest {
        limit: Some(2),
        ..ReadRequest::default()
    };
    let s = convert(
        &r,
        Path::new("in.a"),
        Path::new("out.b"),
        None,
        None,
        &req,
        &WriteRequest::default(),
    )
    .unwrap();
    assert_eq!(s.rows, 2);
    // explicit names override extensions
    let s = convert(
        &r,
        Path::new("in.weird"),
        Path::new("out.weird"),
        Some("alpha"),
        Some("beta"),
        &ReadRequest::default(),
        &WriteRequest::default(),
    )
    .unwrap();
    assert_eq!(s.rows, 3);
}

#[test]
fn read_only_formats_refuse_to_write() {
    let slot = Arc::new(Mutex::new(vec![table()]));
    let mut r = Registry::new();
    r.register(Arc::new(Memory {
        name: "ro",
        ext: &["ro"],
        slot,
        writable: false,
    }));
    let err = convert(
        &r,
        Path::new("a.ro"),
        Path::new("b.ro"),
        None,
        None,
        &ReadRequest::default(),
        &WriteRequest::default(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Unsupported(_)), "{err}");
}
