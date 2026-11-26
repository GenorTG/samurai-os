// The complete list of privileged operations the toolbox can ask for.
//
// This is a security boundary. `samurai-gui-helper` runs as root through pkexec and executes only
// what `build` returns: a fixed argument vector for the `samurai` CLI. The GUI uses the same table
// to show the equivalent command. Standard library only, so the root binary links no Qt.
#pragma once

#include <optional>
#include <string>
#include <vector>

namespace samurai::ops {

/// Arguments after `samurai` for `verb` with `args`. Empty optional (and `error` set) when the verb
/// is unknown, the argument count is wrong or any argument fails validation.
std::optional<std::vector<std::string>> build(const std::string &verb,
                                              const std::vector<std::string> &args,
                                              std::string &error);

/// Every verb `build` accepts, in table order.
std::vector<std::string> verbs();

}  // namespace samurai::ops
