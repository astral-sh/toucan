#include <git2.h>

/* These helpers use the C compiler's field access, independently of Toucan's layout. */
unsigned int toucan_git_commit_allow_empty(const git_commit_create_options *options) {
    return options->allow_empty_commit;
}

void toucan_git_commit_set_allow_empty(git_commit_create_options *options, unsigned int value) {
    options->allow_empty_commit = value;
}
