# MySQL: `Item_hex_string::val_int`, 8.0.26 and 8.0.27

Number read from a hexadecimal or bit literal (construct `hex_long_zero` of
`crates/dialects/reference/mysql.yaml`). No entry of the release notes of
8.0.27 describes the change.

Sources:

- https://github.com/mysql/mysql-server/blob/mysql-8.0.26/sql/item.cc (line 6610)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.27/sql/item.cc (line 6625)

Both versions, but the return value of a literal of more than 8 bytes whose
bytes before the last 8 are not all zero: `-1` in 8.0.26 (2^64 - 1 read as
unsigned), `0` in 8.0.27, after the warning 1292 (`ER_TRUNCATED_WRONG_VALUE`).

```cpp
longlong Item_hex_string::val_int() {
  // following assert is redundant, because fixed=1 assigned in constructor
  assert(fixed == 1);
  const char *end = str_value.ptr() + str_value.length();
  const char *ptr;

  if (str_value.length() > sizeof(longlong)) {
    /*
      Too many bytes for longlong; lost bytes are [start, lost_end[ ; there is
      no loss of data in conversion only if they are all zeroes.
    */
    const char *lost_end = end - sizeof(longlong);
    for (ptr = str_value.ptr(); ptr < lost_end; ++ptr)
      if (*ptr != 0) {
        // [the literal, written into errbuff]
        push_warning_printf(
            thd, Sql_condition::SL_WARNING, ER_TRUNCATED_WRONG_VALUE,
            ER_THD(thd, ER_TRUNCATED_WRONG_VALUE), "BINARY", errbuff);
        return -1;  // 8.0.26; `return 0;` in 8.0.27
      }
  }

  ptr = end - str_value.length();
  ulonglong value = 0;
  for (; ptr != end; ptr++) value = (value << 8) + (ulonglong)(uchar)*ptr;
  return (longlong)value;
}
```
