# MariaDB: type of `IF`, `NVL2` and `IFNULL`

Type of the result of `IF`, `NVL2` and `IFNULL` (`NVL` is `IFNULL`,
`Create_func_ifnull`), compared with `CASE` (`crates/dialects/src/build/flow.rs`,
measures `crates/probe/cases/flow-types.txt`). The code is the same in 10.6 and
11.8.

Sources:

- https://github.com/MariaDB/server/blob/10.6/sql/item_cmpfunc.h (lines 1242, 1297, 1379, 1416)
- https://github.com/MariaDB/server/blob/11.8/sql/item_cmpfunc.h (lines 1295, 1340, 1422, 1459)
- https://github.com/MariaDB/server/blob/11.8/sql/item_create.cc (lines 6423, 6498, 6499)

`IF` and `NVL2` merge the types of their two results, as `CASE` does, but a
result that is the `NULL` literal gives the type of the other one, whole:
`IF(c, u, NULL)` of an `INT UNSIGNED` is an `INT UNSIGNED`, where `CASE` gives
a `DECIMAL(10,0)`. `IFNULL` merges its two arguments, without this exception.

```cpp
// 11.8, Item_func_case_abbreviation2
  bool fix_length_and_dec2_eliminate_null(Item **items)
  {
    // Let IF(cond, expr, NULL) and IF(cond, NULL, expr) inherit type from expr.
    if (items[0]->type() == NULL_ITEM)
    {
      cache_type_info(items[1], true);
      // If both arguments are NULL, make resulting type BINARY(0).
      if (items[1]->type() == NULL_ITEM)
        set_handler(&type_handler_string);
    }
    else if (items[1]->type() == NULL_ITEM)
    {
      cache_type_info(items[0], true);
    }
    else
    {
      if (fix_length_and_dec2(items))
        return TRUE;
    }
    return FALSE;
  }
```

```cpp
// 11.8, Item_func_if::fix_length_and_dec and Item_func_nvl2::fix_length_and_dec
    return fix_length_and_dec2_eliminate_null(args + 1);

// 11.8, Item_func_ifnull::fix_length_and_dec
    update_nullability_post_fix_fields();
    if (Item_func_case_abbreviation2::fix_length_and_dec2(args))
      return TRUE;
```
