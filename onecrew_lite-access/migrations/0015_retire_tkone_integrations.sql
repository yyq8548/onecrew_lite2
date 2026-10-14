-- These five tables belonged to the retired TkOne/TikTok Business integration.
-- Children before referenced parents; DROP TABLE removes their owned indexes.
DROP TABLE IF EXISTS oauth_authorization_transaction;
DROP TABLE IF EXISTS integration_revocation;
DROP TABLE IF EXISTS desktop_integration_authorization;
DROP TABLE IF EXISTS integration_read_request;
DROP TABLE IF EXISTS third_party_connection;
