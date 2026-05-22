// Package server wires routes and middleware onto a chi.Router.
package server

import (
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/redis/go-redis/v9"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/handler"
	"github.com/logos-design/logos/backend-go/internal/storage"
)

// Deps groups all dependencies required to build the server.
type Deps struct {
	Pool       *db.Pool
	Redis      *redis.Client   // may be nil (cache disabled)
	Storage    storage.Backend // may be nil (photo uploads disabled)
	AuthMW     *auth.Middleware
}

// New returns an http.Handler with all routes registered.
func New(deps Deps) http.Handler {
	r := chi.NewRouter()

	// Standard middleware
	r.Use(middleware.RequestID)
	r.Use(middleware.RealIP)
	r.Use(middleware.Logger)
	r.Use(middleware.Recoverer)
	r.Use(middleware.StripSlashes)

	// CORS — allow the frontend origin in development
	r.Use(corsMiddleware)

	// Session authentication (non-blocking: populates ctx, does not reject).
	if deps.AuthMW != nil {
		r.Use(deps.AuthMW.Handler)
	}

	// ── Health ───────────────────────────────────────────────────────────────
	r.Get("/api/_health", handler.Health)

	// ── RPC commands ─────────────────────────────────────────────────────────
	r.Route("/api/rpc/command", func(r chi.Router) {
		// Profile
		r.Get("/get-profile", handler.ProfileHandler(deps.Pool, deps.Redis))
		r.Patch("/update-profile", handler.UpdateProfileHandler(deps.Pool, deps.Redis))
		r.Patch("/update-profile-props", handler.UpdateProfilePropsHandler(deps.Pool, deps.Redis))
		r.Post("/update-profile-photo", handler.UpdateProfilePhotoHandler(deps.Pool, deps.Redis, deps.Storage))
		r.Delete("/delete-profile", handler.DeleteProfileHandler(deps.Pool, deps.Redis))

		// Teams
		r.Get("/get-teams", handler.GetTeamsHandler(deps.Pool, deps.Redis))
		r.Get("/get-team", handler.GetTeamHandler(deps.Pool, deps.Redis))
		r.Post("/create-team", handler.CreateTeamHandler(deps.Pool, deps.Redis))
		r.Patch("/update-team", handler.UpdateTeamHandler(deps.Pool, deps.Redis))
		r.Delete("/delete-team", handler.DeleteTeamHandler(deps.Pool, deps.Redis))
		r.Post("/leave-team", handler.LeaveTeamHandler(deps.Pool, deps.Redis))
		r.Get("/get-team-members", handler.GetTeamMembersHandler(deps.Pool))
		r.Get("/get-team-stats", handler.GetTeamStatsHandler(deps.Pool))
		r.Get("/get-team-invitations", handler.GetTeamInvitationsHandler(deps.Pool))
		r.Post("/update-team-member-role", handler.UpdateTeamMemberRoleHandler(deps.Pool, deps.Redis))
		r.Delete("/delete-team-member", handler.DeleteTeamMemberHandler(deps.Pool, deps.Redis))

		// Team Invitations
		r.Post("/create-team-invitations", handler.CreateTeamInvitationsHandler(deps.Pool))
		r.Delete("/delete-team-invitation", handler.DeleteTeamInvitationHandler(deps.Pool))
		r.Patch("/update-team-invitation-role", handler.UpdateTeamInvitationRoleHandler(deps.Pool))

		// Projects
		r.Get("/get-projects", handler.GetProjectsHandler(deps.Pool))
		r.Get("/get-project", handler.GetProjectHandler(deps.Pool))
		r.Post("/create-project", handler.CreateProjectHandler(deps.Pool))
		r.Patch("/rename-project", handler.RenameProjectHandler(deps.Pool))
		r.Delete("/delete-project", handler.DeleteProjectHandler(deps.Pool))
		r.Post("/update-project-pin", handler.UpdateProjectPinHandler(deps.Pool))

		// Files (metadata)
		r.Get("/get-file", handler.GetFileHandler(deps.Pool))
		r.Get("/get-project-files", handler.GetProjectFilesHandler(deps.Pool))
		r.Get("/get-file-libraries", handler.GetFileLibrariesHandler(deps.Pool))
		r.Get("/get-file-collaborators", handler.GetFileCollaboratorsHandler(deps.Pool))
		r.Patch("/update-file-metadata", handler.UpdateFileMetadataHandler(deps.Pool))

		// Files — creation
		r.Post("/create-file", handler.CreateFileHandler(deps.Pool))
		r.Post("/duplicate-file", handler.DuplicateFileHandler(deps.Pool))

		// Files — share links
		r.Post("/create-share-link", handler.CreateShareLinkHandler(deps.Pool))
		r.Delete("/delete-share-link", handler.DeleteShareLinkHandler(deps.Pool))
		r.Get("/get-share-link", handler.GetShareLinkHandler(deps.Pool))

		// Viewer (unauthenticated; accepts share-id or session)
		r.Get("/get-view-only-bundle", handler.GetViewOnlyBundleHandler(deps.Pool))
	})

	return r
}

// corsMiddleware adds permissive CORS headers for local development.
// Replace with a stricter policy before production use.
func corsMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
		w.Header().Set("Access-Control-Allow-Headers", "Content-Type, X-Profile-Id")

		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		next.ServeHTTP(w, r)
	})
}
