package api

import (
	"context"
	"crypto/ed25519"
	"encoding/base64"
	"github.com/jackc/pgx/v5"
	"net/http"
)

// Every sensitive writer takes the same account lock and rechecks authorization after
// waiting. Revocation/rotation cannot race with a request authenticated beforehand.
func (s *Server) authorizeTx(ctx context.Context, tx pgx.Tx, i identity, approved bool) error {
	if _, e := tx.Exec(ctx, "SELECT id FROM users WHERE id=$1 FOR UPDATE", i.Account); e != nil {
		return e
	}
	var valid bool
	e := tx.QueryRow(ctx, `SELECT EXISTS(SELECT 1 FROM sessions s JOIN devices d ON d.account_id=s.account_id AND d.id=s.device_id WHERE s.id=$1 AND s.account_id=$2 AND s.device_id=$3 AND NOT s.revoked AND s.access_expires>now() AND d.status<>'revoked' AND (NOT $4 OR d.status='approved'))`, i.Session, i.Account, i.Device, approved).Scan(&valid)
	if e != nil {
		return e
	}
	if !valid {
		return pgx.ErrNoRows
	}
	return nil
}
func (s *Server) rotate(w http.ResponseWriter, r *http.Request, i identity) {
	var in struct {
		BaseEpoch int    `json:"baseEpoch"`
		Revoke    string `json:"revokeDevice"`
		Recovery  string `json:"recoveryEnvelope"`
		Verifier  string `json:"recoveryVerifier"`
		Devices   []struct {
			ID       string `json:"id"`
			Envelope string `json:"envelope"`
			Proof    string `json:"deviceProof"`
		} `json:"devices"`
		Objects []struct {
			ID         string `json:"id"`
			Kind       string `json:"kind"`
			Base       int64  `json:"baseRevision"`
			Payload    string `json:"payload"`
			Deleted    bool   `json:"deleted"`
			Generation int64  `json:"generation"`
		} `json:"objects"`
	}
	if !decode(w, r, &in, 360<<20) {
		return
	}
	verifier, e := base64.StdEncoding.DecodeString(in.Verifier)
	if e != nil || len(verifier) != ed25519.PublicKeySize || len(in.Recovery) < 16 || len(in.Recovery) > 16384 || in.BaseEpoch < 1 || !validID(in.Revoke) || in.Revoke == i.Device || len(in.Devices) > 100 || len(in.Objects) > 1000 {
		failure(w, 400, "invalid_rotation")
		return
	}
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	if s.authorizeTx(r.Context(), tx, i, true) != nil {
		failure(w, 401, "unauthorized")
		return
	}
	var epoch int
	if tx.QueryRow(r.Context(), "SELECT epoch FROM vaults WHERE account_id=$1 FOR UPDATE", i.Account).Scan(&epoch) != nil || epoch != in.BaseEpoch {
		failure(w, 409, "vault_changed")
		return
	}
	var target bool
	if tx.QueryRow(r.Context(), "SELECT EXISTS(SELECT 1 FROM devices WHERE account_id=$1 AND id=$2 AND status<>'revoked')", i.Account, in.Revoke).Scan(&target) != nil || !target {
		failure(w, 409, "device_changed")
		return
	}
	rows, e := tx.Query(r.Context(), "SELECT id::text FROM devices WHERE account_id=$1 AND status='approved' AND id<>$2", i.Account, in.Revoke)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	expected := map[string]bool{}
	for rows.Next() {
		var id string
		if rows.Scan(&id) != nil {
			rows.Close()
			failure(w, 503, "database_unavailable")
			return
		}
		expected[id] = true
	}
	err := rows.Err()
	rows.Close()
	if err != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	if len(expected) != len(in.Devices) {
		failure(w, 409, "devices_changed")
		return
	}
	for _, d := range in.Devices {
		if !validProof(d.Proof) || !expected[d.ID] || len(d.Envelope) < 16 || len(d.Envelope) > 16384 {
			failure(w, 409, "devices_changed")
			return
		}
		delete(expected, d.ID)
	}
	rows, e = tx.Query(r.Context(), "SELECT id::text,kind,revision,deleted,generation FROM objects WHERE account_id=$1", i.Account)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	objects := map[string]object{}
	for rows.Next() {
		var o object
		if rows.Scan(&o.ID, &o.Kind, &o.Revision, &o.Deleted, &o.Generation) != nil {
			rows.Close()
			failure(w, 503, "database_unavailable")
			return
		}
		objects[o.ID+":"+o.Kind] = o
	}
	err = rows.Err()
	rows.Close()
	if err != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	if len(objects) != len(in.Objects) {
		failure(w, 409, "revision_conflict")
		return
	}
	var size int64
	for _, o := range in.Objects {
		old, ok := objects[o.ID+":"+o.Kind]
		if !ok || old.Revision != o.Base || old.Deleted != o.Deleted || old.Generation != o.Generation || len(o.Payload) == 0 || len(o.Payload) > 12<<20 {
			failure(w, 409, "revision_conflict")
			return
		}
		size += int64(len(o.Payload))
		delete(objects, o.ID+":"+o.Kind)
	}
	if size > 256<<20 {
		failure(w, 413, "quota_exceeded")
		return
	}
	for _, o := range in.Objects {
		if _, e = tx.Exec(r.Context(), "UPDATE objects SET payload=$4,epoch=$5,revision=revision+1,writer=$6,updated_at=now() WHERE account_id=$1 AND id=$2 AND kind=$3", i.Account, o.ID, o.Kind, o.Payload, epoch+1, i.Device); e != nil {
			failure(w, 503, "database_unavailable")
			return
		}
	}
	for _, d := range in.Devices {
		if _, e = tx.Exec(r.Context(), "UPDATE devices SET envelope=$3,epoch=$4,proof=$5 WHERE account_id=$1 AND id=$2", i.Account, d.ID, d.Envelope, epoch+1, d.Proof); e != nil {
			failure(w, 503, "database_unavailable")
			return
		}
	}
	_, e = tx.Exec(r.Context(), "UPDATE vaults SET epoch=$2,recovery_envelope=$3,recovery_verifier=$4 WHERE account_id=$1", i.Account, epoch+1, in.Recovery, in.Verifier)
	if e == nil {
		_, e = tx.Exec(r.Context(), "UPDATE devices SET status='revoked',envelope='' WHERE account_id=$1 AND id=$2", i.Account, in.Revoke)
	}
	if e == nil {
		_, e = tx.Exec(r.Context(), "UPDATE sessions SET revoked=true WHERE account_id=$1 AND device_id=$2", i.Account, in.Revoke)
	}
	if e == nil {
		_, e = tx.Exec(r.Context(), "UPDATE leases SET generation=generation+1,expires_at=now() WHERE account_id=$1", i.Account)
	}
	if e == nil {
		_, e = tx.Exec(r.Context(), "DELETE FROM recovery_challenges WHERE account_id=$1", i.Account)
	}
	if e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	s.audit(r.Context(), i, "vault_rotated_device_revoked", in.Revoke)
	reply(w, 200, map[string]any{"epoch": epoch + 1})
}
