# MariaDB: `PERIOD_ADD` and `PERIOD_DIFF`, 10.6 and 11.8

Reading of the periods (`YYMM` or `YYYYMM`) and of the months (rule
`function.period` of `docs/DEV-DIALECTS.md`, measures
`crates/probe/cases/period.txt`, `period-arguments.txt` and
`period-forms.sql`). The servers measured run on Linux x86-64, where `ulong`
and `long` are 64-bit integers, `int` and `uint` 32-bit ones. The code is the
same in 10.6 and 11.8.

Sources:

- https://github.com/MariaDB/server/blob/10.6/sql/item_timefunc.cc (lines 783, 798)
- https://github.com/MariaDB/server/blob/11.8/sql/item_timefunc.cc (lines 808, 823)
- https://github.com/MariaDB/server/blob/11.8/sql/sql_time.cc (lines 272, 286; 10.6 alike)

No period is refused: a period of 0 or above 999912 counts 0 months, and
`PERIOD_ADD` of a period of 0 gives 0. The month of the period is not checked
(`PERIOD_DIFF(202413, 202401)` gives 12, an error in MySQL). A year below 70
is 20YY, below 100 19YY, as in MySQL.

```cpp
// sql/sql_time.cc
ulong convert_period_to_month(ulong period)
{
  ulong a,b;
  if (period == 0 || period > 999912)
    return 0L;
  if ((a=period/100) < YY_PART_YEAR)
    a+=2000;
  else if (a < 100)
    a+=1900;
  b=period%100;
  return a*12+b-1;
}


ulong convert_month_to_period(ulong month)
{
  ulong year;
  if (month == 0L)
    return 0L;
  if ((year=month/12) < 100)
  {
    year+=(year < YY_PART_YEAR) ? 2000 : 1900;
  }
  return year*100+month%12+1;
}
```

The months are an `int`, truncated to 32 bits, and their sum with the months of
the period is computed as an `int`, then read as an `uint`: a negative sum
wraps around.

```cpp
// sql/item_timefunc.cc
longlong Item_func_period_add::val_int()
{
  DBUG_ASSERT(fixed());
  ulong period=(ulong) args[0]->val_int();
  int months=(int) args[1]->val_int();

  if ((null_value=args[0]->null_value || args[1]->null_value) ||
      period == 0L)
    return 0; /* purecov: inspected */
  return (longlong)
    convert_month_to_period((uint) ((int) convert_period_to_month(period)+
				    months));
}


longlong Item_func_period_diff::val_int()
{
  DBUG_ASSERT(fixed());
  ulong period1=(ulong) args[0]->val_int();
  ulong period2=(ulong) args[1]->val_int();

  if ((null_value=args[0]->null_value || args[1]->null_value))
    return 0; /* purecov: inspected */
  return (longlong) ((long) convert_period_to_month(period1)-
		     (long) convert_period_to_month(period2));
}
```
