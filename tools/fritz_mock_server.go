package main

import (
	"encoding/json"
	"encoding/xml"
	"log"
	"log/slog"
	"net/http"
	"os"
)

var (
	loginDefaultSid       = "0000000000000000"
	loginSidSuccess       = "4827051936271849"
	loginDefaultChallenge = "00000000"
	loginChallengeSuccess = "59372618"
	loginDefaultBlockTime = 0
)

func xmlResponse(w http.ResponseWriter, data any) {
	w.Header().Set("Content-Type", "application/xml")
	w.WriteHeader(http.StatusOK)
	xml.NewEncoder(w).Encode(data)
}

func jsonResponse(w http.ResponseWriter, data any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	json.NewEncoder(w).Encode(data)
}

func mockHandleLoginSidLua(w http.ResponseWriter, r *http.Request) {
	// Not reall testing just passing through..
	type SessionInfo struct {
		SID       string
		Challenge string
		BlockTime int
	}
	switch r.Method {
	case http.MethodGet:
		xmlResponse(w, SessionInfo{loginDefaultSid, loginDefaultChallenge, loginDefaultBlockTime})
	case http.MethodPost:
		xmlResponse(w, SessionInfo{loginSidSuccess, loginChallengeSuccess, loginDefaultBlockTime})
	}
}

func main() {
	slog := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{AddSource: true}))
	http.HandleFunc("/login_sid.lua", mockHandleLoginSidLua)
	addr := ":8000"
	slog.Info("Starting at : ", "Address", addr)
	log.Fatal(http.ListenAndServe(addr, nil))
}
