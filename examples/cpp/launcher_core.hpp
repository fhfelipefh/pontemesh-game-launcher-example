#pragma once

#include <algorithm>
#include <cctype>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <set>
#include <stdexcept>
#include <string>
#include <vector>

#include <nlohmann/json.hpp>

namespace launcher {

constexpr std::uint64_t max_release_bytes = 20ULL * 1024 * 1024 * 1024;
constexpr std::size_t max_release_files = 10'000;

struct ReleaseFile {
    std::string bucket;
    std::string key;
    std::filesystem::path path;
    std::uint64_t size_bytes;
    std::string sha256;
    int order;
};

struct Release {
    std::string version;
    std::vector<ReleaseFile> files;
};

inline bool is_hex_digest(const std::string& value) {
    return value.size() == 64 && std::all_of(value.begin(), value.end(), [](unsigned char character) {
        return std::isxdigit(character) != 0;
    });
}

inline std::filesystem::path safe_path(const std::string& value) {
    if (value.empty() || value.find('\\') != std::string::npos) {
        throw std::runtime_error("release path must be a non-empty portable path");
    }
    std::filesystem::path result(value);
    if (result.is_absolute()) {
        throw std::runtime_error("release path must be relative");
    }
    for (const auto& part : result) {
        if (part == "..") {
            throw std::runtime_error("release path cannot contain parent traversal");
        }
    }
    return result;
}

inline Release validate_manifest(const nlohmann::json& value) {
    if (!value.is_object() || value.value("schemaVersion", 0) != 1) {
        throw std::runtime_error("release descriptor must use schemaVersion 1");
    }
    if (!value.contains("version") || !value["version"].is_string() || value["version"].get<std::string>().empty()) {
        throw std::runtime_error("release version cannot be empty");
    }
    if (!value.contains("files") || !value["files"].is_array() || value["files"].empty() || value["files"].size() > max_release_files) {
        throw std::runtime_error("release must contain between 1 and 10,000 files");
    }
    Release release{value["version"].get<std::string>(), {}};
    std::set<std::filesystem::path> seen;
    std::uint64_t total = 0;
    for (const auto& item : value["files"]) {
        for (const auto* field : {"bucket", "key", "path", "sha256"}) {
            if (!item.contains(field) || !item[field].is_string() || item[field].get<std::string>().empty()) {
                throw std::runtime_error(std::string("release file ") + field + " cannot be empty");
            }
        }
        if (!item.contains("sizeBytes") ||
            (!item["sizeBytes"].is_number_unsigned() &&
             (!item["sizeBytes"].is_number_integer() || item["sizeBytes"].get<std::int64_t>() < 0))) {
            throw std::runtime_error("sizeBytes must be a non-negative integer");
        }
        const auto relative = safe_path(item["path"].get<std::string>());
        if (!seen.insert(relative).second) {
            throw std::runtime_error("release paths must be unique");
        }
        const auto digest = item["sha256"].get<std::string>();
        if (!is_hex_digest(digest)) {
            throw std::runtime_error("sha256 must contain 64 hexadecimal characters");
        }
        const auto size = item["sizeBytes"].get<std::uint64_t>();
        if (size > max_release_bytes - total) {
            throw std::runtime_error("release exceeds the 20 GiB example limit");
        }
        total += size;
        release.files.push_back({
            item["bucket"].get<std::string>(),
            item["key"].get<std::string>(),
            relative,
            size,
            digest,
            item.value("order", 0),
        });
    }
    std::stable_sort(release.files.begin(), release.files.end(), [](const ReleaseFile& left, const ReleaseFile& right) {
        return left.order < right.order;
    });
    return release;
}

inline void replace_installation(const std::filesystem::path& install_root, const std::filesystem::path& staging) {
    const auto rollback = std::filesystem::path(install_root.string() + ".rollback");
    std::filesystem::remove_all(rollback);
    if (std::filesystem::exists(install_root)) {
        std::filesystem::rename(install_root, rollback);
    }
    try {
        std::filesystem::rename(staging, install_root);
    } catch (...) {
        if (std::filesystem::exists(rollback)) {
            std::filesystem::rename(rollback, install_root);
        }
        throw;
    }
    std::filesystem::remove_all(rollback);
}

inline nlohmann::json read_json(const std::filesystem::path& path) {
    std::ifstream input(path);
    if (!input) throw std::runtime_error("could not read " + path.string());
    return nlohmann::json::parse(input);
}

}
