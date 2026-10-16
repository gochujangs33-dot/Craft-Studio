# Craft-Studio (ArtCraft Studio)

> **13종 무료 오픈소스 크리에이티브 & 오피스 프로그램 통합 런처 및 자동 업데이트 관리자**  
> GitHub Repository: [https://github.com/gochujangs33-dot/Craft-Studio](https://github.com/gochujangs33-dot/Craft-Studio)

---

## 🌟 주요 기능 및 아키텍처

1. **단 하나의 런처로 13종 오픈소스 스위트 완전 통합**
   - 사진, 벡터, 영상, 오디오, 3D/CAD, 전자출판, 오피스, UI/UX 등 13종의 순수 Rust 기반 고성능 프로그램을 하나의 런처에서 원클릭으로 실행합니다.
2. **3M 스타일 '동일 위치 대체(In-place Replace)' 업데이트 지원**
   - 상단 바의 **[🔄 업데이트]** 버튼으로 통합 업데이트 관리자를 호출할 수 있습니다.
   - 새 릴리스가 발견되면 별도의 설치 마법사 없이 기존 설치 폴더에 실행 파일 및 라이브러리를 **그대로 덮어쓰기 대체**합니다.
   - **사용자 작업 보존**: 사용자 프로젝트와 작업물이 저장되는 `SavedFiles/` 폴더는 앱 바이너리와 완전히 격리되어 있어 업데이트 시에도 100% 안전하게 유지됩니다.
3. **독립 다중 창(Multi-Window) 실행**
   - 각 프로그램은 항상 독립된 새 프로세스/창으로 실행되어 여러 창을 동시에 띄워두고 멀티태스킹 작업을 수행할 수 있습니다.
4. **반응형 대시보드 & 모니터 해상도 자동 맞춤**
   - 실행 시 자동으로 창이 최대화(Maximized)되어 FHD, QHD, 4K, 노트북 모니터 및 윈도우 배율(125%/150%)에 상관없이 카드가 잘리지 않고 꽉 차게 정렬됩니다.
   - 마우스 커서를 올리는 즉시 대기 시간(`0.0초`) 없이 프로그램 상세 설명 툴팁이 팝업됩니다.
5. **다국어 완벽 지원 (한국어 / English)**
   - 윈도우 맑은 고딕(`malgun.ttf`) 폰트 엔진 내장으로 깨끗한 한글 렌더링을 제공하며, 상단 바에서 즉시 언어를 전환할 수 있습니다.
6. **오픈소스 커스텀 모드 지원 (`custom_apps/`)**
   - 사용자가 직접 코드를 수정한 프로그램은 `<exe_dir>/custom_apps/<id>/<id>.exe`에 넣으면 런처가 보라색 `[MOD]` 태그를 달고 우선 실행합니다.

---

## 📁 13종 프로그램 구성표

| # | 프로그램 | 주요 기능 | 상용 대체 툴 | 지원 포맷 |
|:---:|:---|:---|:---|:---|
| **01** | **PhotoCraft** | 전문가급 래스터 이미지 및 사진 편집 | Adobe Photoshop | `.psd`, `.png`, `.jpg`, `.webp`, `.tif` |
| **02** | **VectorCraft** | 벡터 일러스트레이션 및 그래픽 디자인 | Adobe Illustrator | `.svg`, `.ai`, `.eps` |
| **03** | **FilmCraft** | 타임라인 비선형 영상 편집 및 색보정 | Adobe Premiere Pro | `.mp4`, `.mov`, `.mkv`, `.webm` |
| **04** | **LightCraft** | 비파괴 사진 라이브러리 및 RAW 현상 | Adobe Lightroom | `.dng`, `.cr2`, `.nef`, `.arw`, `.raw` |
| **05** | **PdfCraft** | PDF 열람, 조판, 주석 및 보안 관리 | Adobe Acrobat | `.pdf` |
| **06** | **EffectCraft** | 모션 그래픽, VFX 시각 효과 및 모션 | Adobe After Effects | `.aep`, `.lottie` |
| **07** | **DesignCraft** | 다단 페이지 레이아웃, 조판 및 전자 출판 | Adobe InDesign | `.indd`, `.idml` |
| **08** | **WordCraft** | 문서 작성, 정형 서식 및 워드프로세서 | Microsoft Word | `.docx`, `.doc`, `.rtf`, `.odt`, `.txt` |
| **09** | **DeckCraft** | 슬라이드 쇼 및 프레젠테이션 제작 | Microsoft PowerPoint | `.pptx`, `.ppt`, `.odp`, `.key` |
| **10** | **GridCraft** | 스프레드시트, 표 계산 수식 및 데이터 분석 | Microsoft Excel | `.xlsx`, `.xls`, `.csv`, `.ods`, `.tsv` |
| **11** | **CADCraft** | 2D/3D 컴퓨터 지원 도면 설계 및 제도 | Autodesk AutoCAD | `.dwg`, `.dxf` |
| **12** | **SoundCraft** | 멀티트랙 오디오 녹음, 사운드 편집 및 믹싱 | Avid Pro Tools | `.wav`, `.mp3`, `.flac`, `.ogg`, `.ptx` |
| **13** | **FigCraft** | UI/UX 인터페이스 디자인 및 프로토타이핑 | Figma | `.fig` |

---

## 🚀 빠른 시작 가이드

### 1. 일반 사용자 실행
1. [GitHub Releases](https://github.com/gochujangs33-dot/Craft-Studio/releases)에서 최신 `ArtCraftStudio-portable-x64.zip`을 다운로드합니다.
2. 압축을 해제한 뒤 `ArtCraftStudio.exe`를 더블 클릭하여 실행합니다.
3. 작업한 모든 프로젝트 파일은 `SavedFiles/` 폴더에 프로그램별로 자동 보관됩니다.

### 2. 개발 및 빌드 (Rust 환경)
```powershell
# 1. 런처 빌드 및 dist 번들 조립
powershell -ExecutionPolicy Bypass -File .\scripts\build-all.ps1

# 2. 독립형 포터블 ZIP 패키징
powershell -ExecutionPolicy Bypass -File .\scripts\package.ps1

# 3. GitHub 릴리스 배포용 파일 준비 (release/upload_assets)
powershell -ExecutionPolicy Bypass -File .\scripts\prepare-release-assets.ps1
```

---

## 🤖 AI 에이전트(Claude Code, Codex) 작업 안내

* 본 프로젝트는 Claude Code 및 Codex 등의 AI 에이전트가 완벽하게 연속 작업할 수 있도록 설계되었습니다.
* 세부 아키텍처 규칙, 불변 조건, 모듈 맵 및 확장 방법은 **[CLAUDE.md](file:///c:/Users/0op64/.gemini/antigravity/scratch/ArtCraft/CLAUDE.md)** 및 **[AGENTS.md](file:///c:/Users/0op64/.gemini/antigravity/scratch/ArtCraft/AGENTS.md)**에 표준화되어 있습니다.
