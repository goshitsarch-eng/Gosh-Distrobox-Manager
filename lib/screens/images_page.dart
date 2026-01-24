import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';

class ImagesPage extends StatefulWidget {
  const ImagesPage({super.key});

  @override
  State<ImagesPage> createState() => _ImagesPageState();
}

class _ImagesPageState extends State<ImagesPage> {
  final TextEditingController _customImageController = TextEditingController();
  final TextEditingController _searchController = TextEditingController();
  String _searchQuery = '';

  @override
  void initState() {
    super.initState();
    // Load available images when page opens
    WidgetsBinding.instance.addPostFrameCallback((_) {
      context.read<AppStateProvider>().loadAvailableImages();
    });
  }

  @override
  void dispose() {
    _customImageController.dispose();
    _searchController.dispose();
    super.dispose();
  }

  List<String> _filterImages(List<String> images) {
    if (_searchQuery.isEmpty) return images;
    final query = _searchQuery.toLowerCase();
    return images.where((image) => image.toLowerCase().contains(query)).toList();
  }

  String _extractImageName(String imageUrl) {
    // Extract a friendly name from the image URL
    // e.g., "registry.fedoraproject.org/fedora:39" -> "Fedora"
    final parts = imageUrl.split('/');
    final lastPart = parts.last.split(':').first;
    if (lastPart.isEmpty) return 'Unknown';
    // Capitalize first letter
    return lastPart[0].toUpperCase() + lastPart.substring(1);
  }

  String _extractImageTag(String imageUrl) {
    // Extract the tag from the image URL
    // e.g., "registry.fedoraproject.org/fedora:39" -> "39"
    final parts = imageUrl.split(':');
    if (parts.length > 1) {
      return parts.last;
    }
    return 'latest';
  }

  IconData _getDistroIcon(String imageName) {
    final nameLower = imageName.toLowerCase();
    if (nameLower.contains('ubuntu')) return Icons.circle;
    if (nameLower.contains('fedora')) return Icons.filter_vintage;
    if (nameLower.contains('arch')) return Icons.architecture;
    if (nameLower.contains('debian')) return Icons.donut_large;
    if (nameLower.contains('alpine')) return Icons.landscape;
    if (nameLower.contains('centos') || nameLower.contains('rocky')) {
      return Icons.shield;
    }
    if (nameLower.contains('opensuse') || nameLower.contains('suse')) {
      return Icons.pets;
    }
    if (nameLower.contains('gentoo')) return Icons.build;
    if (nameLower.contains('void')) return Icons.blur_on;
    return Icons.dns;
  }

  Color _getDistroColor(String imageName) {
    final nameLower = imageName.toLowerCase();
    if (nameLower.contains('ubuntu')) return Colors.orange;
    if (nameLower.contains('fedora')) return Colors.blue;
    if (nameLower.contains('arch')) return Colors.cyan;
    if (nameLower.contains('debian')) return Colors.red;
    if (nameLower.contains('alpine')) return Colors.blueGrey;
    if (nameLower.contains('centos') || nameLower.contains('rocky')) {
      return Colors.green;
    }
    if (nameLower.contains('opensuse') || nameLower.contains('suse')) {
      return Colors.lightGreen;
    }
    return Colors.purple;
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Row(
          children: [
            Icon(Icons.album_outlined, color: Color(0xFF137FEC)),
            SizedBox(width: 8),
            Text('Images', style: TextStyle(fontWeight: FontWeight.bold)),
          ],
        ),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            onPressed: () {
              context.read<AppStateProvider>().loadAvailableImages();
            },
          ),
        ],
      ),
      body: Consumer<AppStateProvider>(
        builder: (context, appState, child) {
          return Column(
            children: [
              // Headline
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('Available Images',
                        style: Theme.of(context)
                            .textTheme
                            .headlineMedium
                            ?.copyWith(fontWeight: FontWeight.bold)),
                    const SizedBox(height: 4),
                    Text('Compatible Linux distributions for Distrobox.',
                        style: TextStyle(color: Theme.of(context).hintColor)),
                  ],
                ),
              ),

              // Search Bar
              Padding(
                padding: const EdgeInsets.all(16.0),
                child: SearchBar(
                  controller: _searchController,
                  leading: const Icon(Icons.search),
                  hintText: 'Search images...',
                  onChanged: (value) {
                    setState(() => _searchQuery = value);
                  },
                  elevation: WidgetStateProperty.all(0),
                  backgroundColor: WidgetStateProperty.resolveWith((states) {
                    return Theme.of(context).brightness == Brightness.dark
                        ? Colors.grey[800]
                        : Colors.white;
                  }),
                  shape: WidgetStateProperty.all(RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(12),
                    side: BorderSide(color: Theme.of(context).dividerColor),
                  )),
                ),
              ),

              // Image Grid
              Expanded(
                child: _buildImageContent(context, appState),
              ),

              // Pull Custom Image
              Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const Divider(),
                    const SizedBox(height: 16),
                    const Text('Custom Image URL',
                        style: TextStyle(fontWeight: FontWeight.w600)),
                    const SizedBox(height: 8),
                    Row(
                      children: [
                        Expanded(
                          child: TextField(
                            controller: _customImageController,
                            decoration: const InputDecoration(
                              hintText: 'e.g. docker.io/library/ubuntu:22.04',
                              border: OutlineInputBorder(
                                  borderRadius:
                                      BorderRadius.all(Radius.circular(12))),
                              contentPadding:
                                  EdgeInsets.symmetric(horizontal: 16),
                            ),
                          ),
                        ),
                        const SizedBox(width: 8),
                        FilledButton(
                          onPressed: () {
                            if (_customImageController.text.isNotEmpty) {
                              // Navigate to create container page with this image
                              ScaffoldMessenger.of(context).showSnackBar(
                                SnackBar(
                                  content: Text(
                                      'Use "${_customImageController.text}" in Create Container'),
                                ),
                              );
                            }
                          },
                          style: FilledButton.styleFrom(
                            shape: RoundedRectangleBorder(
                                borderRadius: BorderRadius.circular(12)),
                            padding: const EdgeInsets.symmetric(
                                horizontal: 24, vertical: 16),
                          ),
                          child: const Icon(Icons.arrow_forward),
                        ),
                      ],
                    ),
                    const SizedBox(height: 8),
                    Text(
                      'Enter a custom image URL to use when creating a new container.',
                      style: TextStyle(
                          fontSize: 12, color: Theme.of(context).hintColor),
                    ),
                  ],
                ),
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _buildImageContent(BuildContext context, AppStateProvider appState) {
    if (appState.isLoadingImages) {
      return const Center(child: CircularProgressIndicator());
    }

    if (appState.imagesError != null) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(Icons.error_outline,
                size: 48, color: Theme.of(context).colorScheme.error),
            const SizedBox(height: 16),
            Text(appState.imagesError!,
                textAlign: TextAlign.center,
                style: TextStyle(color: Theme.of(context).colorScheme.error)),
            const SizedBox(height: 16),
            FilledButton.icon(
              onPressed: () => appState.loadAvailableImages(),
              icon: const Icon(Icons.refresh),
              label: const Text('Retry'),
            ),
          ],
        ),
      );
    }

    final filteredImages = _filterImages(appState.availableImages);

    if (filteredImages.isEmpty) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(Icons.album_outlined,
                size: 64, color: Theme.of(context).hintColor),
            const SizedBox(height: 16),
            Text(
              _searchQuery.isEmpty
                  ? 'No images available'
                  : 'No images match your search',
              style: TextStyle(color: Theme.of(context).hintColor),
            ),
          ],
        ),
      );
    }

    return GridView.builder(
      gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
        crossAxisCount: 2,
        mainAxisSpacing: 16,
        crossAxisSpacing: 16,
        childAspectRatio: 1.0,
      ),
      padding: const EdgeInsets.all(16),
      itemCount: filteredImages.length,
      itemBuilder: (context, index) {
        final image = filteredImages[index];
        return _buildImageCard(context, image);
      },
    );
  }

  Widget _buildImageCard(BuildContext context, String imageUrl) {
    final name = _extractImageName(imageUrl);
    final tag = _extractImageTag(imageUrl);
    final icon = _getDistroIcon(imageUrl);
    final color = _getDistroColor(imageUrl);

    return InkWell(
      onTap: () {
        // Show image details or navigate to create container with this image
        _showImageDetailsDialog(context, imageUrl, name, tag);
      },
      borderRadius: BorderRadius.circular(16),
      child: Container(
        decoration: BoxDecoration(
          color: Theme.of(context).cardColor,
          borderRadius: BorderRadius.circular(16),
          border: Border.all(color: Theme.of(context).dividerColor),
        ),
        child: Stack(
          children: [
            Center(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Container(
                    width: 64,
                    height: 64,
                    decoration: BoxDecoration(
                      color: color.withOpacity(0.1),
                      borderRadius: BorderRadius.circular(16),
                    ),
                    child: Icon(icon, size: 32, color: color),
                  ),
                  const SizedBox(height: 12),
                  Text(name, style: const TextStyle(fontWeight: FontWeight.bold)),
                  Text(tag,
                      style: TextStyle(
                          fontSize: 12, color: Theme.of(context).hintColor)),
                ],
              ),
            ),
            Positioned(
              top: 8,
              right: 8,
              child: IconButton(
                icon: Icon(Icons.add_circle_outline, color: color),
                tooltip: 'Create container with this image',
                onPressed: () {
                  _showImageDetailsDialog(context, imageUrl, name, tag);
                },
              ),
            ),
          ],
        ),
      ),
    );
  }

  void _showImageDetailsDialog(
      BuildContext context, String imageUrl, String name, String tag) {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(name),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _buildDetailRow('Tag', tag),
            const SizedBox(height: 8),
            _buildDetailRow('Image URL', imageUrl),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Close'),
          ),
          FilledButton.icon(
            onPressed: () {
              Navigator.pop(context);
              // Navigate to create container page with this image pre-selected
              // For now, just show a snackbar
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(
                  content: Text('Navigate to Create Container with $name'),
                  action: SnackBarAction(
                    label: 'Go',
                    onPressed: () {
                      // TODO: Navigate to create container page with image
                    },
                  ),
                ),
              );
            },
            icon: const Icon(Icons.add),
            label: const Text('Create Container'),
          ),
        ],
      ),
    );
  }

  Widget _buildDetailRow(String label, String value) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label,
            style: TextStyle(
                fontSize: 12,
                color: Theme.of(context).hintColor,
                fontWeight: FontWeight.bold)),
        const SizedBox(height: 2),
        SelectableText(value, style: const TextStyle(fontSize: 14)),
      ],
    );
  }
}
