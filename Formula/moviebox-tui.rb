class MovieboxTui < Formula
  VERSION = "0.1.25"
  MACOS_SHA256 = "e1db6ba4cc0638d42d0af27951305702a9b66e58c055ee6ac6e508347ed28263"
  LINUX_X64_SHA256 = "43f4729109d7daf356260b5afb5aa58d5c00ca94f294887e61c3a94c7cdae308"
  LINUX_ARM64_SHA256 = "8d00177eac8b1d0ec3f14d728a7a064dfdd3a187c822aabbbfb5251a1a4a2ae5"

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
