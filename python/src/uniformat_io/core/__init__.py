"""UniFormat core: format registry and read / write / convert API over the Arrow data model.

A *format* is an object with the attributes of :class:`Format`. Format distributions register theirs in the
``uniformat_io.formats`` entry-point group; :func:`formats` discovers them lazily. This module never imports a
format package by name.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, Iterable, List, Optional, Tuple, Union

__all__ = [
    "Format",
    "FormatNotFoundError",
    "UniFormatError",
    "UniFormatWarning",
    "convert",
    "formats",
    "get_format",
    "read",
    "register",
    "schema",
    "unregister",
    "write",
]

PathLike = Union[str, "os.PathLike[str]"]
ENTRY_POINT_GROUP = "uniformat_io.formats"


class UniFormatError(Exception):
    """Base class of every error raised by UniFormat packages."""


class FormatNotFoundError(UniFormatError):
    """No registered format matches the requested name or file extension."""


class UniFormatWarning(UserWarning):
    """Base class of the warnings emitted by UniFormat packages."""


@dataclass
class Format:
    """Description of a file format.

    ``read_table(path, **options) -> pyarrow.Table`` is required. ``write_table(path, table, **options)`` and
    ``schema(path, **options) -> pyarrow.Schema`` are optional.
    """

    name: str
    extensions: Tuple[str, ...]
    read_table: Callable[..., Any]
    write_table: Optional[Callable[..., Any]] = None
    schema: Optional[Callable[..., Any]] = None
    metadata: Optional[Callable[..., Any]] = None
    description: str = ""
    extra: Dict[str, Any] = field(default_factory=dict)

    def matches(self, path: PathLike) -> bool:
        ext = os.path.splitext(os.fspath(path))[1].lstrip(".").lower()
        return bool(ext) and ext in (e.lstrip(".").lower() for e in self.extensions)


_registry: Dict[str, Format] = {}
_discovered = False


def register(fmt: Format) -> Format:
    """Registers (or replaces) a format."""
    _registry[fmt.name.lower()] = fmt
    return fmt


def unregister(name: str) -> None:
    """Removes a format from the registry (no error if absent)."""
    _registry.pop(name.lower(), None)


def _discover() -> None:
    global _discovered
    if _discovered:
        return
    _discovered = True
    try:
        from importlib.metadata import entry_points
    except ImportError:  # pragma: no cover
        return
    eps = entry_points()
    selected = eps.select(group=ENTRY_POINT_GROUP) if hasattr(eps, "select") else eps.get(ENTRY_POINT_GROUP, [])
    for ep in selected:
        try:
            fmt = ep.load()
        except Exception as exc:  # a broken optional format must not hide the others
            import warnings

            warnings.warn(f"cannot load format {ep.name!r}: {exc}", UniFormatWarning, stacklevel=3)
            continue
        if isinstance(fmt, Format) and fmt.name.lower() not in _registry:
            register(fmt)


def formats() -> List[str]:
    """Names of the registered formats."""
    _discover()
    return sorted(_registry)


def get_format(name_or_path: PathLike, *, by: str = "auto") -> Format:
    """The format called ``name_or_path`` or, for a path, the format that owns its extension."""
    _discover()
    key = os.fspath(name_or_path)
    if by in ("auto", "name") and key.lower() in _registry:
        return _registry[key.lower()]
    if by in ("auto", "path"):
        for fmt in _registry.values():
            if fmt.matches(key):
                return fmt
    raise FormatNotFoundError(f"no registered format for {key!r} (registered: {', '.join(sorted(_registry)) or 'none'})")


def _resolve(path: PathLike, format: Optional[str]) -> Format:
    return get_format(format, by="name") if format else get_format(path, by="path")


def read(path: PathLike, *, format: Optional[str] = None, **options: Any):
    """Reads ``path`` into a ``pyarrow.Table``; the format is chosen by extension unless ``format`` is given."""
    return _resolve(path, format).read_table(os.fspath(path), **options)


def schema(path: PathLike, *, format: Optional[str] = None, **options: Any):
    """Arrow schema of ``path``, read without the data when the format allows it."""
    fmt = _resolve(path, format)
    if fmt.schema is None:
        raise UniFormatError(f"format {fmt.name!r} cannot report a schema without reading the data")
    return fmt.schema(os.fspath(path), **options)


def write(path: PathLike, data: Any, *, format: Optional[str] = None, **options: Any):
    """Writes ``data`` (an Arrow-compatible table) to ``path``; the format is chosen by extension."""
    fmt = _resolve(path, format)
    if fmt.write_table is None:
        raise UniFormatError(f"format {fmt.name!r} is read-only")
    return fmt.write_table(os.fspath(path), data, **options)


def convert(
    src: PathLike,
    dst: PathLike,
    *,
    src_format: Optional[str] = None,
    dst_format: Optional[str] = None,
    columns: Optional[Iterable[str]] = None,
    read_options: Optional[Dict[str, Any]] = None,
    write_options: Optional[Dict[str, Any]] = None,
):
    """Converts ``src`` to ``dst`` through the common (Arrow) data model."""
    ropts = dict(read_options or {})
    if columns is not None:
        ropts["columns"] = list(columns)
    table = read(src, format=src_format, **ropts)
    return write(dst, table, format=dst_format, **dict(write_options or {}))
