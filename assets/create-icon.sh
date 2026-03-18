#!/bin/bash

# 创建 macOS App Icon (.icns)
# 需要安装 ImageMagick: brew install imagemagick

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SVG_FILE="${SCRIPT_DIR}/logo.svg"
ICONSET_DIR="${SCRIPT_DIR}/IntercomTags.iconset"
ICNS_FILE="${SCRIPT_DIR}/AppIcon.icns"

echo "创建 macOS 图标..."

# 清理旧文件
rm -rf "${ICONSET_DIR}"
rm -f "${ICNS_FILE}"

# 创建 iconset 目录
mkdir -p "${ICONSET_DIR}"

# 生成各种尺寸的图标
sizes=(16 32 64 128 256 512 1024)

for size in "${sizes[@]}"; do
    # 普通分辨率
    convert -background none -resize "${size}x${size}" "${SVG_FILE}" "${ICONSET_DIR}/icon_${size}x${size}.png"
    
    # Retina 分辨率 (@2x)
    if [ $size -lt 512 ]; then
        retina_size=$((size * 2))
        convert -background none -resize "${retina_size}x${retina_size}" "${SVG_FILE}" "${ICONSET_DIR}/icon_${size}x${size}@2x.png"
    fi
done

# 创建 .icns 文件
iconutil -c icns "${ICONSET_DIR}" -o "${ICNS_FILE}"

# 清理
rm -rf "${ICONSET_DIR}"

echo "✅ 图标创建成功: ${ICNS_FILE}"
echo ""
echo "使用方式:"
echo "1. 将 AppIcon.icns 复制到 Intercom Tags Manager.app/Contents/Resources/"
echo "2. 确保 Info.plist 中 CFBundleIconFile = AppIcon"
