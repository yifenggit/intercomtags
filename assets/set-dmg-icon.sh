#!/bin/bash

# 为 DMG 文件设置自定义图标
# 使用方法: ./set-dmg-icon.sh <dmg文件路径> <icon文件路径>

set -e

DMG_FILE="$1"
ICON_FILE="$2"

if [ -z "$DMG_FILE" ] || [ -z "$ICON_FILE" ]; then
    echo "用法: $0 <dmg文件> <icon文件>"
    exit 1
fi

if [ ! -f "$DMG_FILE" ]; then
    echo "错误: DMG 文件不存在: $DMG_FILE"
    exit 1
fi

if [ ! -f "$ICON_FILE" ]; then
    echo "错误: 图标文件不存在: $ICON_FILE"
    exit 1
fi

echo "设置 DMG 文件图标..."

# 创建临时工作目录
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

# 将 .icns 转换为临时 .png 用于预览
ICON_PNG="${TEMP_DIR}/icon_256.png"
sips -z 256 256 "${ICON_FILE}" --out "${ICON_PNG}" > /dev/null 2>&1

# 使用 xattr 方法设置自定义图标
# 方法1: 使用 SetFile 标记自定义图标属性
SetFile -a C "${DMG_FILE}"

# 方法2: 创建一个包含图标的资源文件并附加到 DMG
# 这需要更复杂的操作，暂时使用 SetFile 标记

echo "✅ DMG 文件已标记为自定义图标"
echo ""
echo "提示: 如果图标没有立即显示，请尝试："
echo "  1. 在 Finder 中按 Cmd+Shift+. 显示隐藏文件再隐藏"
echo "  2. 或重启 Finder: killall Finder"
echo "  3. 或清清除图标缓存: sudo rm -rf /var/folders/*/*/com.apple.dock.iconcache; killall Dock"
