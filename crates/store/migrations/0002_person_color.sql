-- Each person gets a colour for their avatar. Existing people are given distinct
-- ones in the order they were added, so nobody shares a colour after upgrading.
ALTER TABLE person ADD COLUMN color TEXT;
UPDATE person SET color = CASE (SELECT COUNT(*) FROM person p WHERE p.rowid < person.rowid) % 8
    WHEN 0 THEN 'blue'   WHEN 1 THEN 'green'  WHEN 2 THEN 'orange' WHEN 3 THEN 'red'
    WHEN 4 THEN 'purple' WHEN 5 THEN 'pink'   WHEN 6 THEN 'teal'   ELSE 'slate' END;
