import 'package:flutter/material.dart';
import 'package:gosh_distrobox_manager/src/rust/frb_generated.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';
import 'package:gosh_distrobox_manager/screens/home_screen.dart';
import 'package:provider/provider.dart';
import 'package:google_fonts/google_fonts.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  runApp(const MyApp());
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    const primaryColor = Color(0xFF137FEC);
    const backgroundLight = Color(0xFFF6F7F8);
    const backgroundDark = Color(0xFF101922);

    return MultiProvider(
      providers: [
        ChangeNotifierProvider(create: (_) => AppStateProvider()),
      ],
      child: MaterialApp(
        title: 'Gosh Distrobox Manager',
        debugShowCheckedModeBanner: false,
        theme: ThemeData(
          useMaterial3: true,
          brightness: Brightness.light,
          colorScheme: ColorScheme.fromSeed(
            seedColor: primaryColor,
            primary: primaryColor,
            surface: Colors.white,
            background: backgroundLight,
            brightness: Brightness.light,
          ),
          scaffoldBackgroundColor: backgroundLight,
          textTheme: GoogleFonts.interTextTheme(ThemeData.light().textTheme),
          appBarTheme: const AppBarTheme(
            backgroundColor: Colors.white,
            surfaceTintColor: Colors.transparent,
            elevation: 0,
          ),
          navigationRailTheme: const NavigationRailThemeData(
            backgroundColor: backgroundLight,
            indicatorColor: Color(0xFFD0E4FF), // Lighter primary for selection
          ),
        ),
        darkTheme: ThemeData(
          useMaterial3: true,
          brightness: Brightness.dark,
          colorScheme: ColorScheme.fromSeed(
            seedColor: primaryColor,
            primary: primaryColor,
            surface: const Color(0xFF1E293B), // Slate 800
            background: backgroundDark,
            brightness: Brightness.dark,
          ),
          scaffoldBackgroundColor: backgroundDark,
          textTheme: GoogleFonts.interTextTheme(ThemeData.dark().textTheme),
          appBarTheme: const AppBarTheme(
            backgroundColor: backgroundDark,
            surfaceTintColor: Colors.transparent,
            elevation: 0,
          ),
          navigationRailTheme: const NavigationRailThemeData(
            backgroundColor: backgroundDark,
            indicatorColor: Color(0xFF1E3A5F), // Darker primary for selection
          ),
        ),
        themeMode: ThemeMode.system,
        home: const HomeScreen(),
      ),
    );
  }
}
