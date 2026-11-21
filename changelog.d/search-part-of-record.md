Fixed:

- Studio's search counted 64 000 events while running and 24 000 after Stop with nothing to say why: a stopped
  record and a deep one are searched in the part the scope has read. The count now names that part (`24000 in
  300 µs`, in the table's title `of 800 µs` too) and writes `100000+` where the search ended at its limit
  ([guide](docs/guide.md)).
