#pragma once

#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <functional>
#include <stdexcept>
#include <string>
#include <vector>

#include <pontemesh_sdk.h>

#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

namespace pontemesh {

inline std::string library_filename() {
#ifdef _WIN32
    return "pontemesh_sdk.dll";
#elif __APPLE__
    return "libpontemesh_sdk.dylib";
#else
    return "libpontemesh_sdk.so";
#endif
}

inline std::filesystem::path find_library(const std::filesystem::path& repo_root) {
    std::vector<std::filesystem::path> candidates;
    if (const char* configured = std::getenv("PONTEMESH_SDK_LIBRARY")) candidates.emplace_back(configured);
    candidates.push_back(repo_root / "native" / library_filename());
    candidates.push_back(repo_root.parent_path() / "pontemesh-sdk" / "target" / "release" / library_filename());
    for (const auto& candidate : candidates) {
        if (std::filesystem::is_regular_file(candidate)) return std::filesystem::absolute(candidate);
    }
    throw std::runtime_error("Ponte Mesh native library was not found; set PONTEMESH_SDK_LIBRARY or extract an SDK release into native/");
}

class DynamicLibrary {
public:
    explicit DynamicLibrary(const std::filesystem::path& path) {
#ifdef _WIN32
        handle_ = LoadLibraryW(path.wstring().c_str());
#else
        handle_ = dlopen(path.c_str(), RTLD_NOW | RTLD_LOCAL);
#endif
        if (!handle_) throw std::runtime_error("could not load Ponte Mesh SDK library: " + path.string());
    }

    ~DynamicLibrary() {
#ifdef _WIN32
        if (handle_) FreeLibrary(handle_);
#else
        if (handle_) dlclose(handle_);
#endif
    }

    template <typename Function>
    Function symbol(const char* name) const {
#ifdef _WIN32
        auto address = GetProcAddress(handle_, name);
#else
        auto address = dlsym(handle_, name);
#endif
        if (!address) throw std::runtime_error(std::string("missing Ponte Mesh SDK symbol: ") + name);
        return reinterpret_cast<Function>(address);
    }

private:
#ifdef _WIN32
    HMODULE handle_ = nullptr;
#else
    void* handle_ = nullptr;
#endif
};

class Client {
public:
    using Progress = std::function<void(std::uint32_t, std::uint64_t, std::uint64_t, const std::string&)>;

    Client(const std::filesystem::path& library_path, const std::string& origin_url, const std::string& application_token)
        : library_(library_path),
          create_(library_.symbol<decltype(&pontemesh_client_create)>("pontemesh_client_create")),
          sync_(library_.symbol<decltype(&pontemesh_client_sync_object_with_summary_and_progress)>("pontemesh_client_sync_object_with_summary_and_progress")),
          error_(library_.symbol<decltype(&pontemesh_client_get_last_error)>("pontemesh_client_get_last_error")),
          free_(library_.symbol<decltype(&pontemesh_client_free)>("pontemesh_client_free")) {
        const auto status = create_(origin_url.c_str(), application_token.c_str(), &client_);
        if (status != PONTEMESH_OK) throw std::runtime_error("pontemesh_client_create failed");
    }

    ~Client() {
        if (client_) free_(client_);
    }

    Client(const Client&) = delete;
    Client& operator=(const Client&) = delete;

    PontemeshTransferSummary sync_object(
        const std::string& bucket,
        const std::string& key,
        const std::filesystem::path& destination,
        Progress progress = {}) {
        std::filesystem::create_directories(destination.parent_path());
        PontemeshTransferSummary summary{};
        const auto status = sync_(
            client_, bucket.c_str(), key.c_str(), destination.string().c_str(), &summary,
            progress ? &progress_callback : nullptr, progress ? &progress : nullptr);
        if (status != PONTEMESH_OK) throw std::runtime_error(last_error(status));
        return summary;
    }

private:
    static void progress_callback(
        std::uint32_t fragment,
        std::uint64_t downloaded,
        std::uint64_t total,
        const char* source,
        void* user_data) {
        auto* callback = static_cast<Progress*>(user_data);
        try {
            (*callback)(fragment, downloaded, total, source ? source : "UNKNOWN");
        } catch (...) {
        }
    }

    std::string last_error(PontemeshStatus status) const {
        char buffer[2048]{};
        error_(client_, buffer, sizeof(buffer));
        return buffer[0] ? std::string(buffer) : "Ponte Mesh SDK failed with status " + std::to_string(status);
    }

    DynamicLibrary library_;
    decltype(&pontemesh_client_create) create_;
    decltype(&pontemesh_client_sync_object_with_summary_and_progress) sync_;
    decltype(&pontemesh_client_get_last_error) error_;
    decltype(&pontemesh_client_free) free_;
    PontemeshClient* client_ = nullptr;
};

}
