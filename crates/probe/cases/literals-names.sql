-- Names of the columns of literals without alias.
SELECT N'a', _latin1'b', _binary'c', _utf8mb4'd', N'e' 'f', _latin1'g' 'h', 'i' 'j', _latin1 /* x */ 'k', n'l', _LATIN1'm', _latin1 '', N'';
SELECT _binary'c' 'd', _binary 'e' /* x */ 'f', 'g' /* y */ 'h';
