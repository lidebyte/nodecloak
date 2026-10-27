package api

import (
	"net/http"
	"time"
)

const snapshotWireLimit int64 = 96 << 20
const smallSnapshotWireLimit int64 = 12 << 20

// Bound concurrent materialization of large opaque objects in this process.
var snapshotTransfers = make(chan struct{}, 2)

func beginSnapshotTransfer(w http.ResponseWriter, r *http.Request, limit int64) (func(), bool) {
	if r.ContentLength > limit {
		failure(w, http.StatusRequestEntityTooLarge, "request_too_large")
		return nil, false
	}
	if r.Context().Err() != nil {
		failure(w, http.StatusRequestTimeout, "request_timeout")
		return nil, false
	}
	controller := http.NewResponseController(w)
	deadline := time.Now().Add(180 * time.Second)
	_ = controller.SetReadDeadline(deadline)
	_ = controller.SetWriteDeadline(deadline)
	if r.Method != http.MethodGet && r.ContentLength >= 0 && r.ContentLength <= smallSnapshotWireLimit {
		return func() {}, true
	}
	select {
	case snapshotTransfers <- struct{}{}:
		return func() { <-snapshotTransfers }, true
	default:
		failure(w, http.StatusTooManyRequests, "try_later")
		return nil, false
	}
}
