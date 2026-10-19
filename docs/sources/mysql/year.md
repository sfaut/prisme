# MySQL: the type of `YEAR()`, 8.0.14 to 8.1.0

Type of the result of `YEAR()` (rule `function.year` of
`docs/DEV-DIALECTS.md`, measures `crates/probe/cases/year.txt` and
`year-merges.txt`). Its value is the year of the date in every version, 0 to
9999; only its type changes.

Sources:

- https://github.com/mysql/mysql-server/blob/mysql-8.0.14/sql/item_timefunc.h (line 324)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.17/sql/item_timefunc.h (line 331)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.33/sql/item_timefunc.cc (line 1491)
- https://github.com/mysql/mysql-server/blob/mysql-8.1.0/sql/item_timefunc.cc (line 1497)
- https://github.com/mysql/mysql-server/blob/mysql-8.1.0/sql/item.h (line 1669)
- release notes of 8.0.14 (Bug #28562930) and 8.0.17 (Bug #29668676), `docs/sources/mysql/8.0.14.md`
  and `8.0.17.md`

## 8.0.14 to 8.0.16: an unsigned `INT` of 4 digits

The fix of an overflow of `AVG(YEAR(datetime_column))` (Bug #28562930, 8.0.14)
makes the result unsigned:

```cpp
// 8.0.14, sql/item_timefunc.h, class Item_func_year
bool resolve_type(THD *) override {
  fix_char_length(4); /* 9999 */
  unsigned_flag = true;
  maybe_null = true;
  return false;
}
```

8.0.17 reverts it ("A fix for a previous issue caused `YEAR` values to be
treated as unsigned, but this did not allow for possible negative values
arising during calculations involving subtraction", Bug #29668676):

```cpp
// 8.0.17, sql/item_timefunc.h, class Item_func_year
bool resolve_type(THD *) override {
  fix_char_length(5); /* 9999 plus sign */
  maybe_null = true;
  return false;
}
```

## From 8.1.0: a `YEAR`

No entry in the release notes. The result takes the data type `YEAR`, unsigned,
of 4 characters:

```cpp
// 8.1.0, sql/item_timefunc.cc (8.0.33: fix_char_length(5) in place of set_data_type_year())
bool Item_func_year::resolve_type(THD *thd) {
  if (param_type_is_default(thd, 0, 1, MYSQL_TYPE_DATETIME)) return true;
  set_data_type_year();
  set_nullable(true);
  return false;
}

// 8.1.0, sql/item.h
void set_data_type_year() {
  set_data_type(MYSQL_TYPE_YEAR);
  collation.set_numeric();
  fix_char_length(4);  // YYYY
  unsigned_flag = true;
}
```

The value stays that of `val_int()`, the year of the date: a column of the
result of type `YEAR` gives 1 and 1000 as they are, where `CAST(1 AS YEAR)`
gives 2001 and `CAST(1000 AS YEAR)` NULL.
