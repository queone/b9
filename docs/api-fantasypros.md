# FantasyPros Data

`b9 sync` reads the public rankings page and extracts its embedded `ecrData` object. Rows resolve by Yahoo player ID, then unambiguous normalized name and team. Failed or incomplete refreshes retain the last complete ECR snapshot.
