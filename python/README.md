# uniformat-core (Python)

Common API of the UniFormat family: a registry of file formats, `read` / `write` / `schema` / `convert` functions, and
the `UniFormatError` base class. The common data model is Apache Arrow (`pyarrow.Table`).

```python
import uniformat_io.core as uf

table = uf.read("data.sas7bdat", columns=["x"], limit=100)   # format chosen by extension (needs uniformat-sas)
uf.write("copy.sas7bdat", table, compression="char")
uf.convert("data.sas7bdat", "other.sas7bdat", read_options={"missing": "sidecar"})
print(uf.formats())                                            # registered format names
```

Format packages register themselves through the `uniformat_io.formats` entry-point group:

```toml
[project.entry-points."uniformat_io.formats"]
sas = "uniformat_io.sas:FORMAT"
```

`uniformat_io` is a PEP 420 namespace package: `uniformat_io.core` (this package) and `uniformat_io.<format>` (one
distribution per format) install side by side. This package never imports a format package.
