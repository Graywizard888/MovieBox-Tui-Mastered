class MovieboxTui < Formula
  VERSION = "1.0.4"
  MACOS_SHA256 = "e448a850c7bfa02638c26c2af7d79636355f94b3d35ab130a6798e07b85c4ebe"
  LINUX_X64_SHA256 = "bfb97422cd32c8b0196478fc5038c48c7d99c85e7aeadd497f57932d58b4623a"
  LINUX_ARM64_SHA256 = "b7220e637675ce7bd5d8b4e6b0363d5fdf38fca9d33452a466495b7a524cb138"

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
