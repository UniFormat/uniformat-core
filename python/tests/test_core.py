import pyarrow as pa
import pytest

import uniformat_io.core as uf


def memory_format(name, ext, store):
    return uf.Format(
        name=name,
        extensions=(ext,),
        read_table=lambda path, columns=None, **kw: store[path].select(columns) if columns else store[path],
        write_table=lambda path, table, **kw: store.__setitem__(path, table) or {"rows": table.num_rows},
        schema=lambda path, **kw: store[path].schema,
    )


@pytest.fixture(autouse=True)
def clean_registry():
    before = dict(uf._registry)
    yield
    uf._registry.clear()
    uf._registry.update(before)


def test_lookup_read_write_schema_and_convert():
    store = {}
    uf.register(memory_format("alpha", "alp", store))
    uf.register(memory_format("beta", "bet", store))
    t = pa.table({"x": [1.0, None], "s": ["a", "b"]})
    assert uf.write("one.alp", t) == {"rows": 2}
    assert uf.read("one.alp").equals(t)
    assert uf.read("one.alp", columns=["s"]).column_names == ["s"]
    assert uf.schema("one.alp") == t.schema
    uf.convert("one.alp", "two.bet", columns=["x"])
    assert store["two.bet"].column_names == ["x"]
    assert {"alpha", "beta"} <= set(uf.formats())
    assert uf.get_format("BETA").name == "beta"


def test_errors_are_uniformat_errors():
    with pytest.raises(uf.FormatNotFoundError):
        uf.read("file.unknown")
    assert issubclass(uf.FormatNotFoundError, uf.UniFormatError)
    uf.register(uf.Format(name="ro", extensions=("ro",), read_table=lambda p, **k: pa.table({"a": [1]})))
    with pytest.raises(uf.UniFormatError, match="read-only"):
        uf.write("x.ro", pa.table({"a": [1]}))
    with pytest.raises(uf.UniFormatError, match="schema"):
        uf.schema("x.ro")


def test_explicit_format_overrides_extension():
    store = {}
    uf.register(memory_format("alpha", "alp", store))
    uf.write("data.weird", pa.table({"a": [1]}), format="alpha")
    assert uf.read("data.weird", format="alpha").num_rows == 1


def test_core_does_not_import_any_format():
    import subprocess
    import sys

    # Earlier registry tests may legitimately discover an installed format plugin.
    # Check the import boundary in a fresh process, independent of test order.
    result = subprocess.run(
        [sys.executable, "-c", "import sys; import uniformat_io.core; "
         "assert not [m for m in sys.modules if m.startswith('uniformat_io.') and m != 'uniformat_io.core']"],
        capture_output=True, text=True, check=False,
    )
    assert result.returncode == 0, result.stderr
