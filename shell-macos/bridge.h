// C ABI of launcher-core (core/src/lib.rs). Every returned string must be
// released with lc_free.
#pragma once

char *lc_init(const char *options_json);
char *lc_query(const char *query);
char *lc_query_slow(const char *query);
void lc_cancel_slow(void);
void lc_picked(const char *query, const char *uid);
void lc_clipboard_add(const char *text, const char *app);
void lc_refresh(void);
char *lc_settings(void);
char *lc_write_config(const char *config_json);
void lc_free(char *s);
