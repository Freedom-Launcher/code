// Everything about the service is here and in addon.json. To make a plugin for another
// Yggdrasil / authlib-injector service, change these and the manifest (id, name, hosts,
// account_provider), or run `python3 scripts/new-yggdrasil-plugin.py --help`.
window.PROVIDER = {
  label: 'Ely.by',                                          // shown to the player
  apiRoot: 'https://authserver.ely.by/api/authlib-injector', // same as account_provider.yggdrasil (the game's side)
  authApi: 'https://authserver.ely.by/auth',                  // same as account_provider.auth_api (sign-in; Ely.by documents /auth)
  loginLabel: { ru: 'Ник или e-mail', en: 'Nickname or e-mail' },
  twoFactor: 'colon',   // 'colon': ask for a code only when the service asks, send "password:code"; null: none
  sessionHours: 24,     // the core re-checks the token with the service before each launch anyway
};
