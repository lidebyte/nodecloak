package api

import (
	"context"

	"github.com/jackc/pgx/v5"
)

// revokeDeviceMCP runs in the same account-locked transaction as desktop session
// revocation. An empty device revokes all remote clients for the account. Routes
// are the authority for access tokens, refresh tokens and outstanding OAuth codes;
// retaining their revoked state prevents a later device login reviving a grant.
func revokeDeviceMCP(ctx context.Context, tx pgx.Tx, account, device string) error {
	if _, err := tx.Exec(ctx, "UPDATE mcp_routes SET revoked=true WHERE account_id=$1 AND ($2='' OR device_id::text=$2)", account, device); err != nil {
		return err
	}
	_, err := tx.Exec(ctx, `UPDATE mcp_jobs j SET cancel_requested=true,
  state=CASE WHEN j.state='queued' THEN 'cancelled' ELSE j.state END,
  arguments_sealed='',result_sealed='',updated_at=now()
  FROM mcp_routes r WHERE r.id=j.route_id AND r.account_id=$1 AND ($2='' OR r.device_id::text=$2)`, account, device)
	return err
}
