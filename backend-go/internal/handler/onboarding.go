package handler

import "encoding/json"

// skipOnboardingProfileProps marks onboarding as completed so the dashboard
// does not show the multi-step "Help us get to know you" modal.
var skipOnboardingProfileProps = map[string]any{
	"onboarding-viewed":            true,
	"onboarding-questions":         map[string]any{},
	"onboarding-questions-answered": true,
	"release-notes-viewed":         "2.15.3",
}

func mergeOnboardingSkipProps(props map[string]any) map[string]any {
	if props == nil {
		props = make(map[string]any, len(skipOnboardingProfileProps))
	}
	for k, v := range skipOnboardingProfileProps {
		if _, ok := props[k]; !ok {
			props[k] = v
		}
	}
	return props
}

func mustJSON(v any) []byte {
	b, _ := json.Marshal(v)
	return b
}
