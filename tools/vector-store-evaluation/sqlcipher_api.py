"""Small evaluation-only binding to our explicit SQLCipher build, never system SQLite."""
from __future__ import annotations

import ctypes as c
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / "target/vector-store-evaluation"
LIB = WORK / "sqlcipher/libsqlite3.dylib"
api = c.CDLL(str(LIB))
pointer = c.c_void_p


def signature(name, result, *arguments):
    function = getattr(api, name)
    function.restype = result
    function.argtypes = arguments
    return function


open_db = signature("sqlite3_open_v2", c.c_int, c.c_char_p, c.POINTER(pointer), c.c_int, c.c_char_p)
close_db = signature("sqlite3_close", c.c_int, pointer)
key_db = signature("sqlite3_key", c.c_int, pointer, pointer, c.c_int)
errmsg = signature("sqlite3_errmsg", c.c_char_p, pointer)
prepare = signature("sqlite3_prepare_v2", c.c_int, pointer, c.c_char_p, c.c_int, c.POINTER(pointer), c.POINTER(c.c_char_p))
step = signature("sqlite3_step", c.c_int, pointer)
finalize = signature("sqlite3_finalize", c.c_int, pointer)
reset = signature("sqlite3_reset", c.c_int, pointer)
clear = signature("sqlite3_clear_bindings", c.c_int, pointer)
bind_null = signature("sqlite3_bind_null", c.c_int, pointer, c.c_int)
bind_int = signature("sqlite3_bind_int64", c.c_int, pointer, c.c_int, c.c_int64)
bind_float = signature("sqlite3_bind_double", c.c_int, pointer, c.c_int, c.c_double)
bind_text = signature("sqlite3_bind_text", c.c_int, pointer, c.c_int, c.c_char_p, c.c_int, pointer)
bind_blob = signature("sqlite3_bind_blob", c.c_int, pointer, c.c_int, pointer, c.c_int, pointer)
column_count = signature("sqlite3_column_count", c.c_int, pointer)
column_type = signature("sqlite3_column_type", c.c_int, pointer, c.c_int)
column_int = signature("sqlite3_column_int64", c.c_int64, pointer, c.c_int)
column_float = signature("sqlite3_column_double", c.c_double, pointer, c.c_int)
column_text = signature("sqlite3_column_text", pointer, pointer, c.c_int)
column_blob = signature("sqlite3_column_blob", pointer, pointer, c.c_int)
column_bytes = signature("sqlite3_column_bytes", c.c_int, pointer, c.c_int)
enable_loading = signature("sqlite3_enable_load_extension", c.c_int, pointer, c.c_int)
load_extension = signature("sqlite3_load_extension", c.c_int, pointer, c.c_char_p, c.c_char_p, c.POINTER(c.c_char_p))
free = signature("sqlite3_free", None, pointer)
progress_handler = signature("sqlite3_progress_handler", None, pointer, c.c_int, pointer, pointer)
TRANSIENT = pointer(-1)


class Database:
    def deadline(self, seconds=None):
        if seconds is None:
            progress_handler(self.handle,0,None,None)
            self._progress=None
        else:
            finish=time.perf_counter()+seconds
            self._progress=c.CFUNCTYPE(c.c_int,pointer)(lambda _:int(time.perf_counter()>finish))
            progress_handler(self.handle,2000,c.cast(self._progress,pointer),None)

    def __init__(self, path: Path, key: bytes, engine: str, create=True):
        assert engine in ("vec1", "vec0") and len(key) == 32
        self.handle = pointer()
        self.check(open_db(str(path).encode(), c.byref(self.handle), 2 | (4 if create else 0), None))
        try:
            # sqlite3_key consumes the bytes without putting them in SQL text or logs.
            self.check(key_db(self.handle, key, len(key)))
            self.execute("SELECT count(*) FROM sqlite_schema")  # Force key verification.
            self.check(enable_loading(self.handle, 1))
            error = c.c_char_p()
            rc = load_extension(self.handle, str(WORK / (engine + ".dylib")).encode(), None, c.byref(error))
            if error:
                free(c.cast(error, pointer))
            self.check(rc)
            self.check(enable_loading(self.handle, 0))
            self.execute("PRAGMA temp_store=MEMORY")
            self.execute("PRAGMA secure_delete=ON")
            self.execute("PRAGMA journal_mode=WAL")
            self.execute("PRAGMA synchronous=FULL")
            self.execute("PRAGMA wal_autocheckpoint=0")
        except BaseException:
            self.close()
            raise

    def check(self, code):
        if code not in (0, 100, 101):
            raise RuntimeError(errmsg(self.handle).decode(errors="replace"))

    def execute(self, sql, parameters=()):
        statement = self.statement(sql)
        try:
            return statement.execute(parameters)
        finally:
            statement.close()

    def statement(self, sql):
        if not self.handle:
            raise RuntimeError("Closed search lifetime")
        return Statement(self, sql)

    def close(self):
        if self.handle:
            self.check(close_db(self.handle))
            self.handle = pointer()


class Statement:
    def __init__(self, db, sql):
        self.db = db
        self.handle = pointer()
        tail = c.c_char_p()
        db.check(prepare(db.handle, sql.encode(), -1, c.byref(self.handle), c.byref(tail)))

    def execute(self, values=()):
        db = self.db
        if not db.handle:
            raise RuntimeError("Closed search lifetime")
        db.check(reset(self.handle))
        db.check(clear(self.handle))
        for index, value in enumerate(values, start=1):
            if value is None:
                rc = bind_null(self.handle, index)
            elif isinstance(value, int):
                rc = bind_int(self.handle, index, value)
            elif isinstance(value, float):
                rc = bind_float(self.handle, index, value)
            elif isinstance(value, bytes):
                rc = bind_blob(self.handle, index, value, len(value), TRANSIENT)
            else:
                raw = str(value).encode()
                rc = bind_text(self.handle, index, raw, len(raw), TRANSIENT)
            db.check(rc)
        rows = []
        while True:
            rc = step(self.handle)
            if rc == 101:
                return rows
            db.check(rc)
            row = []
            for col in range(column_count(self.handle)):
                kind = column_type(self.handle, col)
                if kind == 1:
                    value = column_int(self.handle, col)
                elif kind == 2:
                    value = column_float(self.handle, col)
                elif kind in (3, 4):
                    data = column_text(self.handle, col) if kind == 3 else column_blob(self.handle, col)
                    value = c.string_at(data, column_bytes(self.handle, col))
                    if kind == 3:
                        value = value.decode()
                else:
                    value = None
                row.append(value)
            rows.append(tuple(row))

    def close(self):
        if self.handle:
            finalize(self.handle)
            self.handle = pointer()
