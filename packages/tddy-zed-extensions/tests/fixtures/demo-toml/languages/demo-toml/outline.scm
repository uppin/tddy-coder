; A table and an array-of-tables element are items named by their key; a pair is an item named by
; its key. A pair inside a table is nested in it, because the table's node encloses the pair's.

(table
  "["
  (_) @name
  "]") @item

(table_array_element
  "[["
  (_) @name
  "]]") @item

(pair
  (_) @name
  "=") @item
