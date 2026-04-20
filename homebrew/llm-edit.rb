# Homebrew formula template for llm-edit.
#
# This file lives here as a TEMPLATE. To publish via Homebrew, copy it to
# a separate tap repo at github.com/<owner>/homebrew-tap under
# `Formula/llm-edit.rb`, then users install with:
#
#   brew install <owner>/tap/llm-edit
#
# After each `git push origin vX.Y.Z` triggers .github/workflows/release.yml
# and produces tarballs + SHA256SUMS on the GitHub release, update the
# `version`, `url`, and `sha256` fields below for each platform. The
# release workflow prints all three sums in its log to make this easy.

class LlmEdit < Formula
  desc "Structured-op merge tool for multi-agent git workflows"
  homepage "https://github.com/dwms/llm-edit"
  version "0.1.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/dwms/llm-edit/releases/download/v#{version}/llm-edit-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_AARCH64_DARWIN_SHA256"
    else
      url "https://github.com/dwms/llm-edit/releases/download/v#{version}/llm-edit-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_X86_64_DARWIN_SHA256"
    end
  end

  on_linux do
    url "https://github.com/dwms/llm-edit/releases/download/v#{version}/llm-edit-#{version}-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "REPLACE_WITH_X86_64_LINUX_SHA256"
  end

  def install
    bin.install "llm-edit"
  end

  test do
    assert_match "Record structured edits", shell_output("#{bin}/llm-edit --help")
  end
end