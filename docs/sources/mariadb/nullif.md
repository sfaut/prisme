# MariaDB: `NULLIF`, its type and its value

Type of `NULLIF` (`crates/dialects/src/build/flow.rs`, measures
`crates/probe/cases/flow-types.txt` and `flow-direct.txt`) and value it gives.

Source:

- https://github.com/MariaDB/server/blob/11.8/sql/item_cmpfunc.cc (lines 2712, 2868, 3024)

`NULLIF(l_expr, r_expr)` keeps a third argument, a copy of the first, which
gives the value and the type: the comparison may replace `args[0]` (by a
conversion of character set, by the propagation of an equality), never
`args[2]`. The result is that of `CASE WHEN l_expr = r_expr THEN NULL ELSE
l_expr END`, of the type of `l_expr`, a date or a time included.

```cpp
// Item_func_nullif::fix_length_and_dec
  /*
    If this is the first invocation of fix_length_and_dec(), create the
    third argument as a copy of the first. This cannot be done before
    fix_fields(), because fix_fields() might replace items,
    for exampe NOT x --> x==0, or (SELECT 1) --> 1.
    See also class Item_func_nullif declaration.
  */
  if (arg_count == 2)
    args[arg_count++]= m_arg0 ? m_arg0 : args[0];
  // [...]
      args[2] is used to return the value. Afrer all transformations
      (e.g. in fix_length_and_dec(), equal field propagation, etc)
      args[2] points to a an Item which preserves the exact data type and
      attributes (e.g. collation) of the original l_expr.
  // [...]
  set_handler(args[2]->type_handler());
  collation.set(args[2]->collation);
  fix_char_length(args[2]->max_char_length());
```

```cpp
// Item_func_nullif::int_op; real_op, str_op, decimal_op, date_op, time_op
// and native_op alike
  if (!compare())
  {
    null_value=1;
    return 0;
  }
  value= args[2]->val_int();
  null_value= args[2]->null_value;
  return value;
```
