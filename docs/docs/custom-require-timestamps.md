---
id: custom-require-timestamps
title: custom-require-timestamps
---

## problem

Tables that do not include fields for when records were created, updated, and deleted make it difficult to track when changes are made to those records.

Additionally, the absense of a deletion timestamp makes it difficult to implement soft deletes, where records remain in the table but are marked as deleted with the existence of a deletion timestamp.

```sql
-- bad
create table my_table(
  id int
);
```

## solution

Add fields `created_at`, `updated_at`, and `deleted_at` with type `timestamptz`.

`created_at` and `updated_at` are not nullable and default to the current timestamp when a record is created.

`deleted_at` is nullable and defaults to being null. When a record is soft deleted, the `deleted_at` field contains the time at which that record was deleted.

```sql
-- good
create table my_table(
  id int,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz,
);
```
