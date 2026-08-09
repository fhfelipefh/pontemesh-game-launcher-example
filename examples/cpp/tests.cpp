#include <cassert>
#include <filesystem>
#include <fstream>
#include <iostream>

#include "launcher_core.hpp"

nlohmann::json manifest() {
    return {
        {"schemaVersion", 1},
        {"version", "1.0.0"},
        {"files", {{{"bucket", "game-updates"}, {"key", "releases/1.0.0/game/update.pak"}, {"path", "game/update.pak"}, {"sizeBytes", 8}, {"sha256", std::string(64, 'a')}, {"order", 10}}}},
    };
}

int main() {
    const auto release = launcher::validate_manifest(manifest());
    assert(release.version == "1.0.0");
    assert(release.files.size() == 1);

    auto traversal = manifest();
    traversal["files"][0]["path"] = "../outside.pak";
    try {
        launcher::validate_manifest(traversal);
        assert(false);
    } catch (const std::runtime_error&) {
    }

    auto duplicate = manifest();
    duplicate["files"].push_back(duplicate["files"][0]);
    try {
        launcher::validate_manifest(duplicate);
        assert(false);
    } catch (const std::runtime_error&) {
    }

    const auto root = std::filesystem::temp_directory_path() / "pontemesh-cpp-test";
    std::filesystem::remove_all(root);
    const auto installed = root / "installed";
    const auto staging = root / "staging";
    std::filesystem::create_directories(installed);
    std::filesystem::create_directories(staging);
    std::ofstream(installed / "old.txt") << "old";
    std::ofstream(staging / "new.txt") << "new";
    launcher::replace_installation(installed, staging);
    assert(!std::filesystem::exists(installed / "old.txt"));
    assert(std::filesystem::exists(installed / "new.txt"));
    std::filesystem::remove_all(root);
    std::cout << "C++ unit tests passed\n";
}
