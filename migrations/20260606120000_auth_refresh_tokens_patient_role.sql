ALTER TABLE auth_refresh_tokens
    DROP CONSTRAINT IF EXISTS auth_refresh_tokens_role_check;

ALTER TABLE auth_refresh_tokens
    ADD CONSTRAINT auth_refresh_tokens_role_check
    CHECK (role IN ('admin', 'hospital', 'patient'));
