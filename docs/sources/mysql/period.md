# MySQL: `PERIOD_ADD` and `PERIOD_DIFF`, 8.0.11 to 8.4.0

Reading of the periods (`YYMM` or `YYYYMM`) and of the months (rule
`function.period` of `docs/DEV-DIALECTS.md`, measures
`crates/probe/cases/period.txt`, `period-arguments.txt` and
`period-forms.sql`). The servers measured run on Linux x86-64, where `ulong`
and `long` are 64-bit integers, `int` and `uint` 32-bit ones.

Sources:

- https://github.com/mysql/mysql-server/blob/mysql-8.0.11/sql/item_timefunc.cc (lines 959, 974)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.11/sql/sql_time.cc (lines 195, 202, 213)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.15/sql/item_timefunc.cc (line 956)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.16/sql/item_timefunc.cc (lines 970, 984)
- https://github.com/mysql/mysql-server/blob/mysql-8.0.16/mysys/my_time.cc (lines 2126, 2138, 2155)
- https://github.com/mysql/mysql-server/blob/mysql-8.4.0/sql/item_timefunc.cc (lines 1056, 1074)
- https://github.com/mysql/mysql-server/blob/mysql-8.4.0/mysys/my_time.cc (lines 2242, 2254, 2271)
- https://github.com/mysql/mysql-server/blob/mysql-8.4.0/include/my_time.h (line 67, `YY_PART_YEAR = 70`)

## Validity of a period

Every version refuses, with the error 1210 (`ER_WRONG_ARGUMENTS`), a period
that is not positive or whose month (`period % 100`) is not between 1 and 12.
From 8.0.16 the period is a `long long`, an `ulong` before:

```cpp
// 8.0.16 and 8.4.0, mysys/my_time.cc; 8.0.11, sql/sql_time.cc, of an ulong
bool valid_period(long long period) {
  if (period <= 0) return false;
  if ((period % 100) == 0) return false;
  if ((period % 100) > 12) return false;
  return true;
}
```

## Months of a period, period of a month

A year below 70 is 20YY, below 100 19YY. 8.0.11 to 8.0.16 compute with
`ulong`, 8.4.0 with `uint64_t`, of the same 64 bits:

```cpp
// 8.4.0, mysys/my_time.cc; 8.0.11 and 8.0.16 of ulong
uint64_t convert_period_to_month(uint64_t period) {
  uint64_t a;
  unsigned b;
  if (period == 0) return 0L;
  if ((a = period / 100) < YY_PART_YEAR)
    a += 2000;
  else if (a < 100)
    a += 1900;
  b = period % 100;
  return a * 12 + b - 1;
}

uint64_t convert_month_to_period(uint64_t month) {
  uint64_t year;
  if (month == 0L) return 0L;
  if ((year = month / 12) < 100) {
    year += (year < YY_PART_YEAR) ? 2000 : 1900;
  }
  return year * 100 + month % 12 + 1;
}
```

## The functions

8.0.11 to 8.0.15 read the period as an `ulong`, the months as an `int`
(truncated to 32 bits):

```cpp
// 8.0.11, sql/item_timefunc.cc
longlong Item_func_period_add::val_int() {
  DBUG_ASSERT(fixed == 1);
  ulong period = (ulong)args[0]->val_int();
  int months = (int)args[1]->val_int();

  if ((null_value = args[0]->null_value || args[1]->null_value))
    return 0; /* purecov: inspected */
  if (!valid_period(period)) {
    my_error(ER_WRONG_ARGUMENTS, MYF(0), func_name());
    return error_int();
  }
  return (longlong)convert_month_to_period(convert_period_to_month(period) +
                                           months);
}

longlong Item_func_period_diff::val_int() {
  DBUG_ASSERT(fixed == 1);
  ulong period1 = (ulong)args[0]->val_int();
  ulong period2 = (ulong)args[1]->val_int();

  if ((null_value = args[0]->null_value || args[1]->null_value))
    return 0; /* purecov: inspected */
  if (!valid_period(period1) || !valid_period(period2)) {
    my_error(ER_WRONG_ARGUMENTS, MYF(0), func_name());
    return error_int();
  }
  return (longlong)((long)convert_period_to_month(period1) -
                    (long)convert_period_to_month(period2));
}
```

From 8.0.16 both are `long long`; the sum of the months is computed as an
unsigned 64-bit integer:

```cpp
// 8.0.16, sql/item_timefunc.cc
longlong Item_func_period_add::val_int() {
  DBUG_ASSERT(fixed == 1);
  longlong period = args[0]->val_int();
  longlong months = args[1]->val_int();

  if ((null_value = args[0]->null_value || args[1]->null_value))
    return 0; /* purecov: inspected */
  if (!valid_period(period)) {
    my_error(ER_WRONG_ARGUMENTS, MYF(0), func_name());
    return error_int();
  }
  return convert_month_to_period(convert_period_to_month(period) + months);
}

longlong Item_func_period_diff::val_int() {
  // [...] longlong period1, period2, the same checks
  return static_cast<longlong>(convert_period_to_month(period1)) -
         static_cast<longlong>(convert_period_to_month(period2));
}
```

8.4.0 reads the second argument only when the first is not `NULL`, and types a
parameter of `PERIOD_DIFF` as a `BIGINT`; the computation is that of 8.0.16.
