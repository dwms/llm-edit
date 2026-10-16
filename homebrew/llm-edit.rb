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

  # No x86_64-apple-darwin bottle yet — Intel Mac runners are starved on
  # CI. Intel users should build from the source checkout.
  depends_on arch: :arm64 if OS.mac?

  on_macos do
    url "https://github.com/dwms/llm-edit/releases/download/v#{version}/llm-edit-#{version}-aarch64-apple-darwin.tar.gz"
    sha256 "648780f3e6e171d813faddfd2b80b5ee1af13aa553c278e7c829feba40ccb19b"
  end

  on_linux do
    url "https://github.com/dwms/llm-edit/releases/download/v#{version}/llm-edit-#{version}-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "be852e687f4fb167bed605e94ede84ea1019161c19fe237c6107a8bd03edad5d"
  end

  def install
    bin.install "llm-edit"
    bin.install "lint_parse" if File.exist?("lint_parse")
  end

  test do
    assert_match "Record structured edits", shell_output("#{bin}/llm-edit --help")
  end
end