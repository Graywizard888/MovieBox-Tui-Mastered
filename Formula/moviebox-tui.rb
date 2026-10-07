class MovieboxTui < Formula
  VERSION = "1.0.5"
  MACOS_SHA256 = "863459ecbd8d58b892e0eaa0843946d849408908ff8c5668da2713f5cff5595d"
  LINUX_X64_SHA256 = "23cd785644b7da8bb7b3f1b2d99ba89c083b96f3f3392841a60142b10287f838"
  LINUX_ARM64_SHA256 = "d86b73a444297abcb9abb73944e0c08e82a6ec97acd80bd70b2e2ed7db9999ff"

  desc "Stream movies, shows, anime, and live TV from your terminal"
  homepage "https://github.com/mesamirh/MovieBox-Tui"
  version VERSION
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    url "https://github.com/mesamirh/MovieBox-Tui/releases/download/v#{VERSION}/MovieBox_macOS_Universal.tar.gz"
    sha256 MACOS_SHA256
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/mesamirh/MovieBox-Tui/releases/download/v#{VERSION}/MovieBox_Linux_arm64.tar.gz"
      sha256 LINUX_ARM64_SHA256
    else
      url "https://github.com/mesamirh/MovieBox-Tui/releases/download/v#{VERSION}/MovieBox_Linux_x64.tar.gz"
      sha256 LINUX_X64_SHA256
    end
  end

  def install
    bin.install "moviebox-tui"
  end

  test do
    system "#{bin}/moviebox-tui", "--version"
  end
end
