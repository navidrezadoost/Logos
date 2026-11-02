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
	"github.com/logos-design/logos/backend-go/internal/transit"
)

// Deps groups all dependencies required to build the server.
type Deps struct {
	Pool       *db.Pool
	Redis      *redis.Client   // may be nil (cache disabled)
	Storage    storage.Backend // may be nil (photo uploads disabled)
	AuthMW     *auth.Middleware
	TokensKey  []byte // derived from LOGOS_SECRET_KEY; nil disables auth-write endpoints
	CookieName string // auth cookie name, e.g. "auth-token"
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

	// ── RPC routes — two mount points:
	//   /api/rpc/command/   plain JSON  (internal / future TS client)
	//   /api/main/methods/  Transit+JSON (compiled ClojureScript frontend)
	r.Route("/api/rpc/command", func(r chi.Router) {
		registerRPC(r, deps)
	})
	r.Route("/api/main/methods", func(r chi.Router) {
		r.Use(transit.Middleware)
		registerRPC(r, deps)
	})

	r.Get("/ws/notifications", handler.NotificationsHandler(deps.Redis))

	// ── Assets (storage object serving) ─────────────────────────────────────
	r.Get("/assets/by-id/{id}", handler.AssetByIDHandler(deps.Pool, deps.Storage))

	return r
}

// registerRPC mounts every RPC command handler onto the given router.
// Called once for the plain-JSON route group and once for the Transit group.
func registerRPC(r chi.Router, deps Deps) {
	// ── Profile ───────────────────────────────────────────────────────────────
	r.Get("/get-profile", handler.ProfileHandler(deps.Pool, deps.Redis))
	r.Post("/get-profile", handler.ProfileHandler(deps.Pool, deps.Redis)) // frontend may POST
	r.Patch("/update-profile", handler.UpdateProfileHandler(deps.Pool, deps.Redis))
	r.Post("/update-profile", handler.UpdateProfileHandler(deps.Pool, deps.Redis))
	r.Patch("/update-profile-props", handler.UpdateProfilePropsHandler(deps.Pool, deps.Redis))
	r.Post("/update-profile-props", handler.UpdateProfilePropsHandler(deps.Pool, deps.Redis))
	r.Post("/update-profile-photo", handler.UpdateProfilePhotoHandler(deps.Pool, deps.Redis, deps.Storage))
	r.Delete("/delete-profile", handler.DeleteProfileHandler(deps.Pool, deps.Redis))
	r.Post("/delete-profile", handler.DeleteProfileHandler(deps.Pool, deps.Redis))

	// ── Teams ─────────────────────────────────────────────────────────────────
	r.Get("/get-teams", handler.GetTeamsHandler(deps.Pool, deps.Redis))
	r.Post("/get-teams", handler.GetTeamsHandler(deps.Pool, deps.Redis))
	r.Get("/get-team", handler.GetTeamHandler(deps.Pool, deps.Redis))
	r.Post("/get-team", handler.GetTeamHandler(deps.Pool, deps.Redis))
	r.Get("/get-team-info", handler.GetTeamInfoHandler(deps.Pool))
	r.Post("/get-team-info", handler.GetTeamInfoHandler(deps.Pool))
	r.Post("/create-team", handler.CreateTeamHandler(deps.Pool, deps.Redis))
	r.Patch("/update-team", handler.UpdateTeamHandler(deps.Pool, deps.Redis))
	r.Post("/update-team", handler.UpdateTeamHandler(deps.Pool, deps.Redis))
	r.Delete("/delete-team", handler.DeleteTeamHandler(deps.Pool, deps.Redis))
	r.Post("/delete-team", handler.DeleteTeamHandler(deps.Pool, deps.Redis))
	r.Post("/leave-team", handler.LeaveTeamHandler(deps.Pool, deps.Redis))
	r.Get("/get-team-members", handler.GetTeamMembersHandler(deps.Pool))
	r.Post("/get-team-members", handler.GetTeamMembersHandler(deps.Pool))
	r.Get("/get-team-stats", handler.GetTeamStatsHandler(deps.Pool))
	r.Post("/get-team-stats", handler.GetTeamStatsHandler(deps.Pool))
	r.Get("/get-team-invitations", handler.GetTeamInvitationsHandler(deps.Pool))
	r.Post("/get-team-invitations", handler.GetTeamInvitationsHandler(deps.Pool))
	r.Post("/update-team-member-role", handler.UpdateTeamMemberRoleHandler(deps.Pool, deps.Redis))
	r.Delete("/delete-team-member", handler.DeleteTeamMemberHandler(deps.Pool, deps.Redis))
	r.Post("/delete-team-member", handler.DeleteTeamMemberHandler(deps.Pool, deps.Redis))

	// ── Team Invitations ──────────────────────────────────────────────────────
	r.Post("/create-team-invitations", handler.CreateTeamInvitationsHandler(deps.Pool))
	r.Delete("/delete-team-invitation", handler.DeleteTeamInvitationHandler(deps.Pool))
	r.Post("/delete-team-invitation", handler.DeleteTeamInvitationHandler(deps.Pool))
	r.Patch("/update-team-invitation-role", handler.UpdateTeamInvitationRoleHandler(deps.Pool))
	r.Post("/update-team-invitation-role", handler.UpdateTeamInvitationRoleHandler(deps.Pool))

	// ── Projects ──────────────────────────────────────────────────────────────
	r.Get("/get-all-projects", handler.GetAllProjectsHandler(deps.Pool))
	r.Post("/get-all-projects", handler.GetAllProjectsHandler(deps.Pool))
	r.Get("/get-projects", handler.GetProjectsHandler(deps.Pool))
	r.Post("/get-projects", handler.GetProjectsHandler(deps.Pool))
	r.Get("/get-project", handler.GetProjectHandler(deps.Pool))
	r.Post("/get-project", handler.GetProjectHandler(deps.Pool))
	r.Post("/create-project", handler.CreateProjectHandler(deps.Pool, deps.Storage))
	r.Patch("/rename-project", handler.RenameProjectHandler(deps.Pool))
	r.Post("/rename-project", handler.RenameProjectHandler(deps.Pool))
	r.Delete("/delete-project", handler.DeleteProjectHandler(deps.Pool))
	r.Post("/delete-project", handler.DeleteProjectHandler(deps.Pool))
	r.Post("/update-project-pin", handler.UpdateProjectPinHandler(deps.Pool))

	// ── Files (metadata) ──────────────────────────────────────────────────────
	r.Get("/get-file", handler.GetFileHandler(deps.Pool))
	r.Post("/get-file", handler.GetFileHandler(deps.Pool))
	r.Get("/get-file-info", handler.GetFileInfoHandler(deps.Pool))
	r.Post("/get-file-info", handler.GetFileInfoHandler(deps.Pool))
	r.Get("/get-file-fragment", handler.GetFileFragmentHandler(deps.Pool))
	r.Post("/get-file-fragment", handler.GetFileFragmentHandler(deps.Pool))
	r.Get("/get-project-files", handler.GetProjectFilesHandler(deps.Pool))
	r.Post("/get-project-files", handler.GetProjectFilesHandler(deps.Pool))
	r.Get("/get-team-recent-files", handler.GetTeamRecentFilesHandler(deps.Pool))
	r.Post("/get-team-recent-files", handler.GetTeamRecentFilesHandler(deps.Pool))
	r.Get("/get-team-shared-files", handler.GetTeamSharedFilesHandler(deps.Pool))
	r.Post("/get-team-shared-files", handler.GetTeamSharedFilesHandler(deps.Pool))
	r.Get("/get-team-deleted-files", handler.GetTeamDeletedFilesHandler(deps.Pool))
	r.Post("/get-team-deleted-files", handler.GetTeamDeletedFilesHandler(deps.Pool))
	r.Get("/get-file-libraries", handler.GetFileLibrariesHandler(deps.Pool))
	r.Post("/get-file-libraries", handler.GetFileLibrariesHandler(deps.Pool))
	r.Get("/get-file-collaborators", handler.GetFileCollaboratorsHandler(deps.Pool))
	r.Post("/get-file-collaborators", handler.GetFileCollaboratorsHandler(deps.Pool))
	r.Patch("/update-file-metadata", handler.UpdateFileMetadataHandler(deps.Pool))
	r.Post("/update-file-metadata", handler.UpdateFileMetadataHandler(deps.Pool))

	// ── Files — creation ──────────────────────────────────────────────────────
	r.Post("/create-file", handler.CreateFileHandler(deps.Pool))
	r.Post("/duplicate-file", handler.DuplicateFileHandler(deps.Pool))
	r.Post("/delete-file", handler.DeleteFileHandler(deps.Pool))
	r.Delete("/delete-file", handler.DeleteFileHandler(deps.Pool))

	// ── Files — share links ───────────────────────────────────────────────────
	r.Post("/create-share-link", handler.CreateShareLinkHandler(deps.Pool))
	r.Delete("/delete-share-link", handler.DeleteShareLinkHandler(deps.Pool))
	r.Post("/delete-share-link", handler.DeleteShareLinkHandler(deps.Pool))
	r.Get("/get-share-link", handler.GetShareLinkHandler(deps.Pool))
	r.Post("/get-share-link", handler.GetShareLinkHandler(deps.Pool))

	// ── Viewer ────────────────────────────────────────────────────────────────
	r.Get("/get-view-only-bundle", handler.GetViewOnlyBundleHandler(deps.Pool))
	r.Post("/get-view-only-bundle", handler.GetViewOnlyBundleHandler(deps.Pool))

	// ── Auth ──────────────────────────────────────────────────────────────────
	r.Post("/login-with-password", handler.LoginHandler(deps.Pool, deps.TokensKey, deps.CookieName))
	r.Post("/logout", handler.LogoutHandler(deps.Pool, deps.CookieName))
	r.Post("/prepare-register-profile", handler.PrepareRegisterHandler(deps.Pool, deps.TokensKey))
	r.Post("/register-profile", handler.RegisterProfileHandler(deps.Pool, deps.TokensKey, deps.CookieName))
	r.Post("/request-profile-recovery", handler.RequestProfileRecoveryHandler(deps.Pool, deps.TokensKey))
	r.Post("/recover-profile", handler.RecoverProfileHandler(deps.Pool, deps.TokensKey))
	r.Post("/get-sso-provider", handler.GetSSOProviderHandler(deps.Pool))

	// ── LDAP ──────────────────────────────────────────────────────────────────
	r.Post("/login-with-ldap", handler.LoginWithLDAPHandler(deps.Pool, deps.TokensKey, deps.CookieName))

	// ── Access tokens ─────────────────────────────────────────────────────────
	r.Post("/create-access-token", handler.CreateAccessTokenHandler(deps.Pool, deps.TokensKey))
	r.Delete("/delete-access-token", handler.DeleteAccessTokenHandler(deps.Pool))
	r.Post("/delete-access-token", handler.DeleteAccessTokenHandler(deps.Pool))
	r.Get("/get-access-tokens", handler.GetAccessTokensHandler(deps.Pool))
	r.Post("/get-access-tokens", handler.GetAccessTokensHandler(deps.Pool))

	// ── Token verification ────────────────────────────────────────────────────
	r.Post("/verify-token", handler.VerifyTokenHandler(deps.Pool, deps.TokensKey, deps.CookieName))

	// ── File updates ──────────────────────────────────────────────────────────
	r.Post("/update-file", handler.UpdateFileHandler(deps.Pool, deps.Redis))

	// ── Comments ──────────────────────────────────────────────────────────────
	r.Get("/get-comment-threads", handler.GetCommentThreadsHandler(deps.Pool))
	r.Post("/get-comment-threads", handler.GetCommentThreadsHandler(deps.Pool))
	r.Get("/get-comments", handler.GetCommentsHandler(deps.Pool))
	r.Post("/get-comments", handler.GetCommentsHandler(deps.Pool))
	r.Post("/create-comment-thread", handler.CreateCommentThreadHandler(deps.Pool))
	r.Post("/create-comment", handler.CreateCommentHandler(deps.Pool))
	r.Patch("/update-comment", handler.UpdateCommentHandler(deps.Pool))
	r.Post("/update-comment", handler.UpdateCommentHandler(deps.Pool))
	r.Patch("/update-comment-thread", handler.UpdateCommentThreadHandler(deps.Pool))
	r.Post("/update-comment-thread", handler.UpdateCommentThreadHandler(deps.Pool))
	r.Delete("/delete-comment", handler.DeleteCommentHandler(deps.Pool))
	r.Post("/delete-comment", handler.DeleteCommentHandler(deps.Pool))
	r.Delete("/delete-comment-thread", handler.DeleteCommentThreadHandler(deps.Pool))
	r.Post("/delete-comment-thread", handler.DeleteCommentThreadHandler(deps.Pool))
	r.Patch("/update-comment-thread-status", handler.UpdateCommentThreadStatusHandler(deps.Pool))
	r.Post("/update-comment-thread-status", handler.UpdateCommentThreadStatusHandler(deps.Pool))
	r.Get("/get-unread-comment-threads", handler.GetUnreadCommentThreadsHandler(deps.Pool))
	r.Post("/get-unread-comment-threads", handler.GetUnreadCommentThreadsHandler(deps.Pool))
	r.Get("/get-profiles-for-file-comments", handler.GetProfilesForFileCommentsHandler(deps.Pool))
	r.Post("/get-profiles-for-file-comments", handler.GetProfilesForFileCommentsHandler(deps.Pool))
	r.Post("/mark-all-threads-as-read", handler.MarkAllThreadsAsReadHandler(deps.Pool))

	// ── Media ─────────────────────────────────────────────────────────────────
	r.Post("/upload-file-media-object", handler.UploadFileMediaObjectHandler(deps.Pool, deps.Storage))
	r.Post("/clone-file-media-object", handler.CloneFileMediaObjectHandler(deps.Pool))
	r.Get("/get-file-media-objects", handler.GetFileMediaObjectsHandler(deps.Pool))
	r.Post("/get-file-media-objects", handler.GetFileMediaObjectsHandler(deps.Pool))

	// ── Fonts ─────────────────────────────────────────────────────────────────
	r.Get("/get-font-variants", handler.GetFontVariantsHandler(deps.Pool))
	r.Post("/get-font-variants", handler.GetFontVariantsHandler(deps.Pool))
	r.Post("/create-font-variant", handler.CreateFontVariantHandler(deps.Pool, deps.Storage))
	r.Patch("/update-font", handler.UpdateFontHandler(deps.Pool))
	r.Post("/update-font", handler.UpdateFontHandler(deps.Pool))
	r.Delete("/delete-font", handler.DeleteFontHandler(deps.Pool))
	r.Post("/delete-font", handler.DeleteFontHandler(deps.Pool))
	r.Delete("/delete-font-variant", handler.DeleteFontVariantHandler(deps.Pool))
	r.Post("/delete-font-variant", handler.DeleteFontVariantHandler(deps.Pool))

	// ── Binfile export/import (.logos v3 ZIP) ─────────────────────────────────
	r.Post("/export-binfile", handler.ExportBinfileHandler(deps.Pool, deps.Storage))
	r.Post("/import-binfile", handler.ImportBinfileHandler(deps.Pool, deps.Storage))

	// ── File thumbnails ───────────────────────────────────────────────────────
	r.Get("/get-file-object-thumbnails", handler.GetFileObjectThumbnailsHandler(deps.Pool))
	r.Post("/get-file-object-thumbnails", handler.GetFileObjectThumbnailsHandler(deps.Pool))
	r.Post("/create-file-object-thumbnail", handler.CreateFileObjectThumbnailHandler(deps.Pool, deps.Storage))
	r.Delete("/delete-file-object-thumbnail", handler.DeleteFileObjectThumbnailHandler(deps.Pool))
	r.Post("/delete-file-object-thumbnail", handler.DeleteFileObjectThumbnailHandler(deps.Pool))
	r.Post("/create-file-thumbnail", handler.CreateFileThumbnailHandler(deps.Pool, deps.Storage))
	r.Get("/get-file-data-for-thumbnail", handler.GetFileDataForThumbnailHandler(deps.Pool))
	r.Post("/get-file-data-for-thumbnail", handler.GetFileDataForThumbnailHandler(deps.Pool))
	r.Get("/get-file-thumbnail", handler.GetFileThumbnailHandler(deps.Pool))
	r.Post("/get-file-thumbnail", handler.GetFileThumbnailHandler(deps.Pool))

	// ── File snapshots ────────────────────────────────────────────────────────
	r.Get("/get-file-snapshots", handler.GetFileSnapshotsHandler(deps.Pool))
	r.Post("/get-file-snapshots", handler.GetFileSnapshotsHandler(deps.Pool))
	r.Post("/create-file-snapshot", handler.CreateFileSnapshotHandler(deps.Pool))
	r.Patch("/update-file-snapshot", handler.UpdateFileSnapshotHandler(deps.Pool))
	r.Post("/update-file-snapshot", handler.UpdateFileSnapshotHandler(deps.Pool))
	r.Delete("/delete-file-snapshot", handler.DeleteFileSnapshotHandler(deps.Pool))
	r.Post("/delete-file-snapshot", handler.DeleteFileSnapshotHandler(deps.Pool))
	r.Post("/restore-file-snapshot", handler.RestoreFileSnapshotHandler(deps.Pool))
	r.Post("/lock-file-snapshot", handler.LockFileSnapshotHandler(deps.Pool))
	r.Post("/unlock-file-snapshot", handler.UnlockFileSnapshotHandler(deps.Pool))

	// ── Webhooks ──────────────────────────────────────────────────────────────
	r.Get("/get-webhooks", handler.GetWebhooksHandler(deps.Pool))
	r.Post("/get-webhooks", handler.GetWebhooksHandler(deps.Pool))
	r.Post("/create-webhook", handler.CreateWebhookHandler(deps.Pool))
	r.Patch("/update-webhook", handler.UpdateWebhookHandler(deps.Pool))
	r.Post("/update-webhook", handler.UpdateWebhookHandler(deps.Pool))
	r.Delete("/delete-webhook", handler.DeleteWebhookHandler(deps.Pool))
	r.Post("/delete-webhook", handler.DeleteWebhookHandler(deps.Pool))

	// ── Search ────────────────────────────────────────────────────────────────
	r.Get("/search-files", handler.SearchFilesHandler(deps.Pool))
	r.Post("/search-files", handler.SearchFilesHandler(deps.Pool))

	// ── Audit ─────────────────────────────────────────────────────────────────
	r.Post("/push-audit-events", handler.PushAuditEventsHandler(deps.Pool))

	// ── Demo ──────────────────────────────────────────────────────────────────
	r.Post("/create-demo-profile", handler.CreateDemoProfileHandler(deps.Pool))

	// ── Feedback ──────────────────────────────────────────────────────────────
	r.Post("/send-user-feedback", handler.SendUserFeedbackHandler(deps.Pool))

	// ── Management ────────────────────────────────────────────────────────────
	r.Get("/get-enabled-flags", handler.GetEnabledFlagsHandler())
	r.Post("/get-enabled-flags", handler.GetEnabledFlagsHandler())
	r.Get("/get-builtin-templates", handler.GetBuiltinTemplatesHandler(deps.Pool))
	r.Post("/get-builtin-templates", handler.GetBuiltinTemplatesHandler(deps.Pool))
	r.Post("/duplicate-project", handler.DuplicateProjectHandler(deps.Pool))
	r.Post("/move-files", handler.MoveFilesHandler(deps.Pool))
	r.Post("/move-project", handler.MoveProjectHandler(deps.Pool))

	r.NotFound(handler.RPCNotFoundHandler())
}

// corsMiddleware adds permissive CORS headers for local development.
func corsMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, PATCH, DELETE, OPTIONS")
		w.Header().Set("Access-Control-Allow-Headers", "*")
		w.Header().Set("Access-Control-Expose-Headers", "Content-Type, Content-Length")

		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		next.ServeHTTP(w, r)
	})
}
