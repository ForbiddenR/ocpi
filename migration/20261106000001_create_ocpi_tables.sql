CREATE TABLE versions (
    version TEXT PRIMARY KEY NOT NULL,
    url TEXT NOT NULL
);

CREATE TABLE credentials (
    id INTEGER PRIMARY KEY,
    token TEXT NOT NULL UNIQUE CHECK (length(CAST(token AS BLOB)) BETWEEN 1 AND 64),
    url TEXT NOT NULL,
    roles_json TEXT NOT NULL CHECK (json_valid(roles_json) AND json_type(roles_json) = 'array')
);
