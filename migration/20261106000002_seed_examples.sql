-- Demonstration data only. Never use this token for real authentication.
INSERT INTO versions (version, url) VALUES
    ('2.1.1', 'https://example.com/ocpi/2.1.1/'),
    ('2.2', 'https://example.com/ocpi/2.2/');

INSERT INTO credentials (token, url, roles_json) VALUES (
    'example-token-not-a-secret',
    'https://example.com/ocpi/versions/',
    '[{"role":"CPO","party_id":"EXA","country_code":"NL","business_details":{"name":"Example Operator"}}]'
);
