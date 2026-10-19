# MySQL: type of `IFNULL`, compared with `COALESCE` and `CASE`

Type of the result of `IFNULL` (`crates/dialects/src/build/flow.rs`, measures
`crates/probe/cases/flow-types.txt` and `flow-direct.txt`). The excerpts are
those of 9.3.0; 8.0.11 merges the types and creates the column of `IFNULL`
alike.

Sources:

- https://github.com/mysql/mysql-server/blob/mysql-8.0.11/sql/item_cmpfunc.cc (lines 2912, 3651)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.11/sql/item_cmpfunc.h (line 1019)
- https://github.com/mysql/mysql-server/blob/mysql-8.1.0/sql/item.h (line 4993)
- https://github.com/mysql/mysql-server/blob/mysql-9.3.0/sql/item.cc (line 615)
- https://github.com/mysql/mysql-server/blob/mysql-9.3.0/sql/item_cmpfunc.cc (lines 3553, 4468)
- https://github.com/mysql/mysql-server/blob/mysql-9.3.0/sql/item_cmpfunc.h (line 1495)
- https://github.com/mysql/mysql-server/blob/mysql-9.3.0/sql/item_func.cc (line 793)

`IFNULL` inherits from `COALESCE` (`class Item_func_ifnull final : public
Item_func_coalesce`): both merge the types of their arguments, the `NULL`
literal aside, as `CASE` does.

```cpp
// 9.3.0, Item_func_coalesce::resolve_type_inner
  if (param_type_uses_non_param(thd)) return true;
  if (aggregate_type(func_name(), args, arg_count)) return true;
```

The merge of integers keeps the largest type, signed when one of them is; a
signed and an unsigned integer of that type give the next one, a `DECIMAL`
beyond a `BIGINT`. An integer literal is a `BIGINT` (`MYSQL_TYPE_LONGLONG`, the
constructors of `Item_int`), as an integer expression:

```cpp
// 9.3.0, Item::aggregate_type
  enum_field_types new_type = real_data_type(items[itemno]);
  uint32 new_length = items[itemno]->max_length;
  bool new_unsigned = items[itemno]->unsigned_flag;
  bool mixed_signs = false;

  // Aggregate the resulting data type and calculate properties for integers
  for (itemno = itemno + 1; itemno < count; itemno++) {
    // Do not aggregate items with NULL type
    if (items[itemno]->data_type() == MYSQL_TYPE_NULL) continue;
    // [...]
    new_type = Field::field_type_merge(left_type, right_type);
    // [...]
    mixed_signs |= (new_unsigned != items[itemno]->unsigned_flag);
    new_length = max<uint32>(new_length, items[itemno]->max_length);
  }
  if (mixed_signs && is_integer_type(new_type)) {
    bool bump_range = false;
    new_unsigned = false;
    for (uint i = 0; i < count; i++)
      bump_range |= (items[i]->unsigned_flag &&
                     (items[i]->data_type() == new_type ||
                      items[i]->data_type() == MYSQL_TYPE_BIT));
    if (bump_range) {
      switch (new_type) {
        case MYSQL_TYPE_TINY:
          new_type = MYSQL_TYPE_SHORT;
        // [...]
        case MYSQL_TYPE_LONGLONG:
          new_type = MYSQL_TYPE_NEWDECIMAL;
```

The column of the result differs. `COALESCE` and `CASE` create it from the
result type, an integer from its number of digits: `COALESCE(i, 0)` of an
`INT` is an `INT`. `IFNULL` creates it from the merged type: `IFNULL(i, 0)` is
a `BIGINT`, `IFNULL(i, i)` an `INT`.

```cpp
// 9.3.0, Item_func::tmp_table_field (COALESCE, CASE)
  switch (result_type()) {
    case INT_RESULT:
      if (this->data_type() == MYSQL_TYPE_YEAR)
        field = new (*THR_MALLOC) Field_year(is_nullable(), item_name.ptr());
      else if (max_length > MY_INT32_NUM_DECIMAL_DIGITS)
        field = new (*THR_MALLOC) Field_longlong(
            max_length, is_nullable(), item_name.ptr(), unsigned_flag);
      else
        field = new (*THR_MALLOC) Field_long(max_length, is_nullable(),
                                             item_name.ptr(), unsigned_flag);
      break;
```

```cpp
// 9.3.0, Item_func_ifnull::tmp_table_field; 8.0.11 with 0 for false
Field *Item_func_ifnull::tmp_table_field(TABLE *table) {
  return tmp_table_field_from_field_type(table, false);
}
```
