class Bonsai < Formula
  desc "Local-first AST context compressor for LLM ingestion"
  homepage "https://github.com/MickyBalladelli/bonsai"
  url "https://github.com/MickyBalladelli/bonsai/archive/refs/tags/v0.5.29.tar.gz"
  sha256 "ad3b80db8bd8f79f042a2279b579086ae09265d9ac4f906bc7cb5cccae66c97e"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/bonsai --version")
  end
end
