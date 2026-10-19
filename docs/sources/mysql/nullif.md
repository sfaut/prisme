# MySQL: `NULLIF`, its type and the constants compared with a column

Type of `NULLIF` (`crates/dialects/src/build/flow.rs`, measures
`crates/probe/cases/flow-types.txt` and `flow-direct.txt`) and value it gives
(rule `function.nullif` of `docs/DEV-DIALECTS.md`, measures
`crates/probe/cases/nullif.txt` and `nullif-convert.txt`).

Sources:

- https://github.com/mysql/mysql-server/blob/mysql-8.0.11/sql/item_cmpfunc.cc (lines 465, 577, 595, 3126, 3164)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.22/sql/item_cmpfunc.cc (line 3419)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.23/sql/item_cmpfunc.cc (line 3458)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.23/sql/field_common_properties.h (line 116)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.24/sql/item_cmpfunc.cc (line 3547)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.24/sql/field_common_properties.h (line 114)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.42/sql/item_cmpfunc.cc (line 3570)
- https://github.com/mysql/mysql-server/blob/mysql-8.1.0/sql/item_cmpfunc.cc (line 3537)
- https://github.com/mysql/mysql-server/blob/mysql-8.4.0/sql/item_cmpfunc.cc (line 3740)
- https://github.com/mysql/mysql-server/blob/mysql-9.3.0/sql/item_cmpfunc.cc (lines 605, 727, 745, 3824, 3867)
- https://github.com/mysql/mysql-server/blob/mysql-9.3.0/sql/field.h (lines 2432, 2786, 3124)
- https://github.com/mysql/mysql-server/blob/mysql-9.3.0/sql/item_func.cc (line 793)
- https://github.com/mysql/mysql-server/blob/mysql-9.3.0/sql/field.cc (line 5756)
- https://github.com/mysql/mysql-server/blob/mysql-9.3.0/mysys/my_time.cc (lines 943, 1455)

## Type: a date or a time becomes a string

`NULLIF` takes the type of its first argument, but a date or a time becomes a
string of the same length: "This class does not implement temporal data
types". From 8.0.11 to 8.0.23, when the first argument gives a string (a
string, a date or a time), the collation of the result is aggregated with that
of the second argument, before the change of type: a byte string as second
argument makes the result a byte string (`NULLIF(s, CAST(s AS BINARY))` of a
`VARCHAR(30)` is a `VARBINARY(120)`). From 8.0.24 the collation of the
comparison is aggregated, the result keeps its own.

```cpp
// 8.0.11, Item_func_nullif::resolve_type; 8.0.22 and 8.0.23,
// resolve_type_inner
  maybe_null = true;
  set_data_type_from_item(args[0]);
  cached_result_type = args[0]->result_type();
  if (cached_result_type == STRING_RESULT &&
      agg_arg_charsets_for_comparison(collation, args, arg_count))
    return true;

  // This class does not implement temporal data types
  if (is_temporal()) set_data_type_string(args[0]->max_length);
```

```cpp
// 8.0.24, Item_func_nullif::resolve_type_inner
  set_nullable(true);
  set_data_type_from_item(args[0]);
  cached_result_type = args[0]->result_type();
  if (cached_result_type == STRING_RESULT &&
      agg_arg_charsets_for_comparison(cmp.cmp_collation, args, arg_count))
    return true;

  // This class does not implement temporal data types
  if (is_temporal()) set_data_type_string(args[0]->max_length);
```

`YEAR` counts among the temporal types from 8.0.24 (Bug #32395335, "Some
internal functions used with temporal column types did not provide proper
handling for `YEAR` values"):

```cpp
// 8.0.24, sql/field_common_properties.h; 8.0.23 without MYSQL_TYPE_YEAR
inline bool is_temporal_type(enum_field_types type) {
  switch (type) {
    case MYSQL_TYPE_TIME:
    case MYSQL_TYPE_DATETIME:
    case MYSQL_TYPE_TIMESTAMP:
    case MYSQL_TYPE_DATE:
    case MYSQL_TYPE_NEWDATE:
    case MYSQL_TYPE_YEAR:
      return true;
    default:
      return false;
  }
}
```

A `YEAR` thus becomes a string of 4 characters, but its result type stays
`INT_RESULT`: its column is an `INT`, of the same value. 8.0.42 aggregates the
collations only for a date or a time; 8.1.0 also reads the result of such a
type as a string, and a `YEAR` gives a `VARCHAR(4)`:

```cpp
// 8.0.42, Item_func_nullif::resolve_type_inner; 8.1.0, 8.4.0 and 9.3.0 with
// the last assignment
  set_nullable(true);
  set_data_type_from_item(args[0]);
  cached_result_type = args[0]->result_type();

  // This class does not implement temporal data types
  if (is_temporal()) {
    set_data_type_string(args[0]->max_length);
    if (agg_arg_charsets_for_comparison(cmp.cmp_collation, args, arg_count))
      return true;
    cached_result_type = STRING_RESULT;  // from 8.1.0
  }
```

```cpp
// 9.3.0, Item_func::tmp_table_field: the column of a function result
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
    // [...]
    case STRING_RESULT:
      return make_string_field(table);
```

## Value: the first argument, after the comparison

`NULLIF` inherits from `Item_bool_func2`, the class of `=`: its comparison
first runs `Item_bool_func2::resolve_type`, which converts a constant compared
with a column to the type of the column, "to get nicer DATE comparisons". The
constant replaces the argument in `args`, and `NULLIF` gives `args[0]`: a
constant first argument is given as converted.

```cpp
// 9.3.0, Item_bool_func2::resolve_type
  // Make a special case of compare with fields to get nicer DATE comparisons
  if (!(thd->lex->is_view_context_analysis())) {
    bool cvt1, cvt2;
    if (convert_constant_arg(thd, args[0], &args[1], &cvt1) ||
        convert_constant_arg(thd, args[1], &args[0], &cvt2))
      return true;
    if (cvt1 || cvt2) return false;
  }
```

```cpp
// 9.3.0, Item_func_nullif::val_int; val_real, val_str, val_decimal and
// val_json alike
longlong Item_func_nullif::val_int() {
  assert(fixed);
  longlong value;
  if (!cmp.compare()) {
    null_value = true;
    return 0;
  }
  value = args[0]->val_int();
  null_value = args[0]->null_value;
  return value;
}
```

The conversion takes place when the other argument is a column (`FIELD_ITEM`)
compared as an integer: `BIGINT` (`Field_longlong`), a date or a time
(`Field_temporal`), `YEAR` (`Field_year`); never for a string compared with a
date or a date and time, 8.0.11 and 9.3.0 alike:

```cpp
// 9.3.0, Item_bool_func2::convert_constant_arg
  if (field->real_item()->type() != FIELD_ITEM) return false;

  Item_field *field_item = (Item_field *)(field->real_item());
  if (field_item->field->can_be_compared_as_longlong() &&
      !(field_item->is_temporal_with_date() &&
        (*item)->result_type() == STRING_RESULT)) {
    if (convert_constant_item(thd, field_item, item, converted)) return true;
```

`convert_constant_item` stores a constant that is not `NULL` in the column,
and keeps the value stored when the store succeeds: the packed value of a time
(`Item_time_with_ref`), of a date (`Item_datetime_with_ref`), the year
(`make_year_constant`), the integer (`Item_int_with_ref`, a `BIGINT` only when
the value stored compares equal to the constant):

```cpp
// 9.3.0, convert_constant_item
  if ((*item)->may_evaluate_const(thd) &&
      !((*item)->data_type() == field_item->data_type() &&
        (*item)->basic_const_item())) {
    // [...]
    if (!(*item)->is_null() &&
        (((rc = (*item)->save_in_field(field, true)) == TYPE_OK) ||
         rc == TYPE_NOTE_TIME_TRUNCATED))  // TS-TODO
    {
      int field_cmp = 0;
      // [...]
      if (field->type() == MYSQL_TYPE_LONGLONG) {
        field_cmp = stored_field_cmp_to_item(thd, field, *item);
      }
      // [...]
      if (0 == field_cmp) {
        Item *tmp =
            field->type() == MYSQL_TYPE_TIME
                ? new Item_time_with_ref(field->decimals(),
                                         field->val_time_temporal(), *item)
                : is_temporal_type_with_date(field->type())
                    ? new Item_datetime_with_ref(field->type(),
                                                 field->decimals(),
                                                 field->val_date_temporal(),
                                                 *item)
                    : field->type() == MYSQL_TYPE_YEAR
                        ? make_year_constant(field)
                        : new Item_int_with_ref(
                              field->type(), field->val_int(), *item,
                              field->is_flag_set(UNSIGNED_FLAG));
        // [...]
        *converted = true;  // Item was replaced
```

`OLD_CMP` is defined just above its branches: the excerpt keeps them alone.
In 8.0.11, a `YEAR` takes the branch of the integers, `Item_int_with_ref` of
the year stored. `NULLIF(1, t)` of a `TIME` column thus gives 16777216, the packed
value of `00:00:01`, where `NULLIF(1, TIME '10:20:30')` and `NULLIF(i, t)`
give 1 and 42.

The store fails, and the constant stays as written, for an integer the column
refuses: a year below 0, from 100 to 1900 or beyond 2155, a time of 60
minutes or seconds or beyond 838:59:59 (but from 10^10, read as a date and
time), a date below 101 but 0. Against a `BIGINT`, an integer of the type of
the column (a literal) is not converted, and a fraction stores another value
(`stored_field_cmp_to_item`): both stay as written.

```cpp
// 9.3.0, sql/field.cc
type_conversion_status Field_year::store(longlong nr, bool) {
  ASSERT_COLUMN_MARKED_FOR_WRITE;
  if (nr < 0 || (nr >= 100 && nr < MIN_YEAR) || nr > MAX_YEAR) {
    *ptr = 0;
    set_warning(Sql_condition::SL_WARNING, ER_WARN_DATA_OUT_OF_RANGE, 1);
    return TYPE_WARN_OUT_OF_RANGE;
  }
  // [...]
  return TYPE_OK;
}

// 9.3.0, mysys/my_time.cc
bool number_to_time(longlong nr, MYSQL_TIME *ltime, int *warnings) {
  if (nr > TIME_MAX_VALUE) {
    /* For huge numbers try full DATETIME, like str_to_time does. */
    if (nr >= 10000000000LL) /* '0001-00-00 00-00-00' */
    {
      const int warnings_backup = *warnings;
      if (number_to_datetime(nr, ltime, 0, warnings) != -1LL) return false;
      *warnings = warnings_backup;
    }
    set_max_time(ltime, false);
    *warnings |= MYSQL_TIME_WARN_OUT_OF_RANGE;
    return true;
  }
  if (nr < -TIME_MAX_VALUE) {
    set_max_time(ltime, true);
    *warnings |= MYSQL_TIME_WARN_OUT_OF_RANGE;
    return true;
  }
  if ((ltime->neg = (nr < 0))) nr = -nr;
  if (nr % 100 >= 60 || nr / 100 % 100 >= 60) /* Check hours and minutes */
  {
    set_zero_time(ltime, MYSQL_TIMESTAMP_TIME);
    *warnings |= MYSQL_TIME_WARN_OUT_OF_RANGE;
    return true;
  }
  // [...]
}

longlong number_to_datetime(longlong nr, MYSQL_TIME *time_res,
                            my_time_flags_t flags, int *was_cut) {
  // [...]
  if (nr == 0LL || nr >= 10000101000000LL) {
    // [...]
    goto ok;
  }
  if (nr < 101) goto err;
  // [...]
```

Once converted, the constant takes the type of the column even of the same
value: `NULLIF(2024, y)` gives a `BIGINT UNSIGNED` in 8.0, a `VARCHAR(22)` from
8.1.0, where MariaDB gives an `INT`.
